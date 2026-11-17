/*
 * syncthing_launcher.suprx: stage 1 of the background sync plugin, loaded by taiHEN into
 * SceShell (`*main` section of tai/config.txt).
 *
 * It stays tiny on purpose: no libc, no heap, one thread with a small stack polling every
 * few seconds. It never holds syncthing-core in memory: when a cycle is due (see policy.c)
 * it loads stage 2 (syncthing_worker.suprx) from its own tai directory, watches it, asks it to
 * abort as soon as a game / app or the Syncthing app starts, and unloads it once its threads
 * ended, so SceShell's memory returns to its baseline between cycles.
 */
#include <psp2/appmgr.h>
#include <psp2/io/fcntl.h>
#include <psp2/io/stat.h>
#include <psp2/kernel/clib.h>
#include <psp2/kernel/modulemgr.h>
#include <psp2/kernel/processmgr.h>
#include <psp2/kernel/threadmgr.h>
#include <psp2/net/netctl.h>

#include "../../include/syncthing_plugin.h"
#include "log.h"
#include "policy.h"
#include "system.h"

/* Poll period while idle: a cycle starts at most this long after it is due. */
#define IDLE_POLL_US (5u * 1000u * 1000u)
/* Poll period while a worker runs: how fast an abort is requested after a game starts. */
#define WORKER_POLL_US (500u * 1000u)
/* plugin.cfg is re-read at most this often while idle (and always before a cycle). */
#define SETTINGS_REFRESH_SECS 30u
/* Interval of the power ticks keeping the console out of auto-standby during a cycle. */
#define POWER_TICK_SECS 10u
/* Time a worker gets to stop after an abort request before the launcher says so in the log
 * (it keeps waiting: unloading a module whose threads still run would crash SceShell). */
#define ABORT_GRACE_SECS 60u
/* How long module_stop waits for the launcher thread. */
#define THREAD_STOP_TIMEOUT_US (10u * 1000u * 1000u)

#define LAUNCHER_THREAD_STACK 0x4000
#define LAUNCHER_THREAD_PRIORITY 0xB0

static SceUID g_thread = -1;
static SceUID g_alive_mutex = -1;
static volatile int g_quit = 0;
static SyncthingWorkerLink g_link;

static int settings_load(StpSettings *out)
{
    static char text[512];
    int len = stp_read_file(STP_PLUGIN_CFG, text, sizeof(text));
    if (len < 0)
        len = stp_read_file(STP_PLUGIN_CFG ".bak", text, sizeof(text));
    if (len < 0)
        return 0;
    stp_parse_settings(text, (size_t)len, out);
    return 1;
}

static void observe(StpObservation *obs)
{
    obs->now_secs = stp_now_secs();
    stp_observe_apps(&obs->app_running, &obs->gui_running);
}

static const char *result_text(int result)
{
    switch (result) {
    case STP_RESULT_COMPLETED: return "completed";
    case STP_RESULT_ABORTED: return "aborted";
    case STP_RESULT_DEADLINE: return "stopped at its time limit";
    case STP_RESULT_DISABLED: return "disabled in plugin.cfg";
    case STP_RESULT_NOTHING_TO_SYNC: return "nothing to synchronize";
    case STP_RESULT_NOT_CONFIGURED: return "the app is not configured yet";
    case STP_RESULT_NO_NETWORK: return "network unavailable";
    case STP_RESULT_NO_MEMORY: return "not enough memory in SceShell";
    case STP_RESULT_BAD_LINK: return "launcher / worker version mismatch";
    case STP_RESULT_NO_THREAD: return "cannot create its thread";
    default: return "unknown result";
    }
}

/* Watches a loaded worker until its cycle is over. Returns 0 once it may be unloaded, -1 if it
 * reported a fatal error (it must then stay loaded). */
static int watch_worker(void)
{
    StpSettings settings;
    StpObservation obs;
    uint32_t last_tick = 0;
    uint32_t abort_at = 0;
    int grace_logged = 0;
    for (;;) {
        uint32_t state = g_link.state;
        if (state == STP_STATE_DONE || state == STP_STATE_FATAL) {
            __sync_synchronize(); /* the worker wrote `result` before `state` */
            return state == STP_STATE_DONE ? 0 : -1;
        }
        sceKernelDelayThread(WORKER_POLL_US);
        observe(&obs);
        if (obs.now_secs - last_tick >= POWER_TICK_SECS) {
            last_tick = obs.now_secs;
            sceKernelPowerTick(SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND);
        }
        if (g_link.abort == 0) {
            int have = settings_load(&settings);
            if (g_quit || stp_must_abort(have ? &settings : 0, &obs)) {
                g_link.abort = 1;
                abort_at = obs.now_secs;
            }
        } else if (!grace_logged && obs.now_secs - abort_at >= ABORT_GRACE_SECS) {
            grace_logged = 1;
            stp_log("worker still stopping after an abort request, waiting for it");
        }
    }
}

/* Loads the worker, waits for its cycle and unloads it. Returns 0 if the cycle ran, 1 if it
 * could not start, -1 on a fatal worker error. */
static int run_cycle(const char *why)
{
    char path[STP_PATH_MAX];
    char msg[160];
    SyncthingWorkerLink *link = &g_link;
    SceUID worker_mutex;
    SceUID modid;
    int status = 0;
    int outcome;

    if (!stp_network_connected()) {
        stp_log("cycle postponed: Wi-Fi is not connected");
        return 1;
    }
    if (stp_find_worker(path, sizeof(path)) < 0) {
        stp_log("cycle skipped: " STP_WORKER_FILE " not found in ux0:tai/ or ur0:tai/");
        return 1;
    }
    sceClibMemset(&g_link, 0, sizeof(g_link));
    g_link.magic = STP_LINK_MAGIC;
    g_link.version = STP_LINK_VERSION;
    g_link.state = STP_STATE_LOADING;

    /* Tells the app a worker may still be writing (it waits before its own first sync). */
    worker_mutex = sceKernelCreateMutex(STP_WORKER_RUNNING_MUTEX, SCE_KERNEL_ATTR_OPENABLE, 1, 0);
    sceClibSnprintf(msg, sizeof(msg), "starting a cycle (%s)", why);
    stp_log(msg);
    modid = sceKernelLoadStartModule(path, sizeof(link), &link, 0, 0, &status);
    if (modid < 0) {
        int result = g_link.state == STP_STATE_LOADING ? g_link.result : 0;
        sceClibSnprintf(msg, sizeof(msg), "cannot start the worker: 0x%08X (%s)",
                        (unsigned)modid, result ? result_text(result) : "load error");
        stp_log(msg);
        if (worker_mutex >= 0)
            sceKernelDeleteMutex(worker_mutex);
        return 1;
    }

    outcome = watch_worker();
    if (outcome < 0) {
        stp_log("the worker failed (out of memory or internal error): background sync is off"
                " until the next reboot");
        if (worker_mutex >= 0)
            sceKernelDeleteMutex(worker_mutex);
        return -1;
    }
    /* module_stop joins the cycle thread before newlib's memory is released; it refuses
     * (and the module stays loaded) while a worker thread still runs. */
    if (sceKernelStopUnloadModule(modid, 0, 0, 0, 0, &status) < 0) {
        stp_log("the worker refused to stop, retrying until it does");
        while (sceKernelStopUnloadModule(modid, 0, 0, 0, 0, &status) < 0)
            sceKernelDelayThread(WORKER_POLL_US);
    }
    if (worker_mutex >= 0)
        sceKernelDeleteMutex(worker_mutex);
    sceClibSnprintf(msg, sizeof(msg), "worker unloaded: %s", result_text(g_link.result));
    stp_log(msg);
    return 0;
}

static int launcher_thread(SceSize args, void *argp)
{
    StpPolicy policy;
    StpSettings settings;
    StpObservation obs;
    int have_settings = 0;
    uint32_t settings_read = 0;
    (void)args;
    (void)argp;

    observe(&obs);
    stp_policy_init(&policy, obs.now_secs);
    stp_log_running_apps("launcher started");
    while (!g_quit) {
        StpDecision decision;
        observe(&obs);
        if (!have_settings || obs.now_secs - settings_read >= SETTINGS_REFRESH_SECS) {
            have_settings = settings_load(&settings);
            settings_read = obs.now_secs;
        }
        decision = stp_policy_tick(&policy, have_settings ? &settings : 0, &obs);
        if (decision != STP_IDLE) {
            /* Re-read: a toggle made in the app since the last refresh is honoured. */
            have_settings = settings_load(&settings);
            settings_read = obs.now_secs;
            if (have_settings && settings.enabled) {
                int ran = run_cycle(decision == STP_START_EXIT_CYCLE ? "after a game" : "periodic");
                if (ran < 0)
                    stp_policy_disable(&policy);
                else
                    stp_policy_cycle_ended(&policy, stp_now_secs(), ran > 0);
            }
        }
        sceKernelDelayThread(IDLE_POLL_US);
    }
    return 0;
}

int module_start(SceSize args, void *argp)
{
    (void)args;
    (void)argp;
    g_alive_mutex = sceKernelCreateMutex(STP_LAUNCHER_ALIVE_MUTEX, SCE_KERNEL_ATTR_OPENABLE, 0, 0);
    g_thread = sceKernelCreateThread("SyncthingLauncher", launcher_thread, LAUNCHER_THREAD_PRIORITY,
                                     LAUNCHER_THREAD_STACK, 0,
                                     SCE_KERNEL_THREAD_CPU_AFFINITY_MASK_DEFAULT, 0);
    if (g_thread >= 0)
        sceKernelStartThread(g_thread, 0, 0);
    return SCE_KERNEL_START_SUCCESS;
}

int module_stop(SceSize args, void *argp)
{
    SceUInt timeout = THREAD_STOP_TIMEOUT_US;
    (void)args;
    (void)argp;
    g_quit = 1;
    if (g_thread >= 0) {
        /* A thread still waiting for a worker keeps the module: never unload it under it. */
        if (sceKernelWaitThreadEnd(g_thread, 0, &timeout) < 0)
            return SCE_KERNEL_STOP_CANCEL;
        sceKernelDeleteThread(g_thread);
        g_thread = -1;
    }
    if (g_alive_mutex >= 0) {
        sceKernelDeleteMutex(g_alive_mutex);
        g_alive_mutex = -1;
    }
    return SCE_KERNEL_STOP_SUCCESS;
}
