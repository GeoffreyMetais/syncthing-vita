/* System queries of the launcher (see system.h). */
#include <psp2/appmgr.h>
#include <psp2/io/fcntl.h>
#include <psp2/io/stat.h>
#include <psp2/kernel/clib.h>
#include <psp2/kernel/threadmgr.h>
#include <psp2/net/netctl.h>

#include "../../include/syncthing_plugin.h"
#include "system.h"

#define TITLE_ID_MAX 32
#define TAI_UX0 "ux0:tai/"
#define TAI_UR0 "ur0:tai/"

uint32_t stp_now_secs(void)
{
    return (uint32_t)(sceKernelGetSystemTimeWide() / 1000000ull);
}

int stp_read_file(const char *path, char *buf, int size)
{
    int len;
    SceUID fd = sceIoOpen(path, SCE_O_RDONLY, 0);
    if (fd < 0)
        return -1;
    len = sceIoRead(fd, buf, (SceSize)(size - 1));
    sceIoClose(fd);
    if (len < 0)
        return -1;
    buf[len] = '\0';
    return len;
}

static int exists(const char *path)
{
    SceIoStat st;
    return sceIoGetstat(path, &st) >= 0;
}

/* Title ID of a running app, or "" when it cannot be read. */
static void title_of(SceInt32 app_id, char *name)
{
    SceUID pid = sceAppMgrGetProcessIdByAppIdForShell(app_id);
    name[0] = '\0';
    if (pid < 0 || sceAppMgrGetNameById(pid, name) < 0)
        name[0] = '\0';
    name[TITLE_ID_MAX - 1] = '\0';
}

void stp_observe_apps(int *app_running, int *gui_running)
{
    SceInt32 ids[STP_MAX_APPS];
    char name[TITLE_ID_MAX];
    SceUID gui_mutex;
    int count = sceAppMgrGetRunningAppIdListForShell(ids, STP_MAX_APPS);
    int i;
    *app_running = 0;
    *gui_running = 0;
    if (count < 0)
        *app_running = 1; /* unknown: never sync blindly */
    for (i = 0; i < count && i < STP_MAX_APPS; i++) {
        title_of(ids[i], name);
        if (sceClibStrncmp(name, STP_GUI_TITLE_ID, sizeof(STP_GUI_TITLE_ID)) == 0)
            *gui_running = 1;
        else
            *app_running = 1;
    }
    /* Defence in depth, also covering a list that would not show the app. */
    gui_mutex = sceKernelOpenMutex(STP_GUI_RUNNING_MUTEX);
    if (gui_mutex >= 0) {
        *gui_running = 1;
        sceKernelCloseMutex(gui_mutex);
    }
}

int stp_running_titles(char *out, int size)
{
    SceInt32 ids[STP_MAX_APPS];
    char name[TITLE_ID_MAX];
    int count = sceAppMgrGetRunningAppIdListForShell(ids, STP_MAX_APPS);
    int used = 0;
    int i;
    out[0] = '\0';
    for (i = 0; i < count && i < STP_MAX_APPS && used < size; i++) {
        title_of(ids[i], name);
        used += sceClibSnprintf(out + used, (SceSize)(size - used), "%s%s", i ? "," : "",
                                name[0] ? name : "?");
    }
    return count;
}

int stp_network_connected(void)
{
    int state = SCE_NETCTL_STATE_DISCONNECTED;
    return sceNetCtlInetGetState(&state) >= 0 && state == SCE_NETCTL_STATE_CONNECTED;
}

static int try_dir(const char *dir, char *out, int size)
{
    sceClibSnprintf(out, (SceSize)size, "%s%s", dir, STP_WORKER_FILE);
    return exists(out) ? 0 : -1;
}

int stp_find_worker(char *out, int size)
{
    int ux0_active = exists(TAI_UX0 "config.txt");
    const char *first = ux0_active ? TAI_UX0 : TAI_UR0;
    const char *second = ux0_active ? TAI_UR0 : TAI_UX0;
    if (try_dir(first, out, size) == 0)
        return 0;
    return try_dir(second, out, size);
}
