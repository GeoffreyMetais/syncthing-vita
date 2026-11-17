/*
 * Contract between the two stages of the background sync plugin and the app.
 *
 * - syncthing_launcher.suprx (stage 1, plugin/launcher): resident in SceShell, decides when
 *   a cycle runs and loads stage 2 for it.
 * - syncthing_worker.suprx (stage 2, crates/syncthing-vita-worker): one sync cycle, then
 *   unloaded. Its Rust mirror of this header is crates/syncthing-vita-worker/src/vita/link.rs
 *   (tests/plugin_contract.rs checks both agree).
 *
 * The launcher passes a pointer to a SyncthingWorkerLink as the argument of the worker's
 * module_start (args = sizeof(SyncthingWorkerLink *)). Both modules live in the SceShell
 * process, so the pointer stays valid for the worker's whole life; the launcher keeps the
 * link alive until the worker is unloaded.
 */
#ifndef SYNCTHING_PLUGIN_H
#define SYNCTHING_PLUGIN_H

#include <stdint.h>

/* Module files, next to each other in the active tai directory. */
#define STP_LAUNCHER_FILE "syncthing_launcher.suprx"
#define STP_WORKER_FILE "syncthing_worker.suprx"

/* Named kernel mutexes (SCE_KERNEL_ATTR_OPENABLE). */
#define STP_GUI_RUNNING_MUTEX "SyncthingGuiRunning"       /* held by the app while it runs */
#define STP_LAUNCHER_ALIVE_MUTEX "SyncthingLauncherAlive" /* exists while the launcher is loaded */
#define STP_WORKER_RUNNING_MUTEX "SyncthingWorkerRunning" /* exists while a worker is loaded */

/* The app's data directory and the plugin settings it writes. */
#define STP_DATA_DIR "ux0:data/syncthing"
#define STP_PLUGIN_CFG STP_DATA_DIR "/plugin.cfg"
#define STP_LOG_FILE STP_DATA_DIR "/syncthing.log"
#define STP_ROTATED_LOG_FILE STP_DATA_DIR "/syncthing.log.1"

#define STP_LINK_MAGIC 0x53544C4Bu /* "STLK" */
#define STP_LINK_VERSION 1u

/* SyncthingWorkerLink.state, written by the worker. */
#define STP_STATE_LOADING 0u /* module_start has not finished */
#define STP_STATE_RUNNING 1u /* the cycle thread runs */
#define STP_STATE_DONE 2u    /* the cycle ended: the module can be stopped and unloaded */
#define STP_STATE_FATAL 3u   /* a worker thread died (out of memory, panic): never unload */

/* SyncthingWorkerLink.result, written by the worker with STP_STATE_DONE. */
#define STP_RESULT_COMPLETED 0
#define STP_RESULT_ABORTED 1
#define STP_RESULT_DEADLINE 2
#define STP_RESULT_DISABLED 3
#define STP_RESULT_NOTHING_TO_SYNC 4
#define STP_RESULT_NOT_CONFIGURED 5
#define STP_RESULT_NO_NETWORK 6
#define STP_RESULT_NO_MEMORY 7   /* module_start: the worker heap could not be allocated */
#define STP_RESULT_BAD_LINK 8    /* module_start: missing or incompatible link */
#define STP_RESULT_NO_THREAD 9   /* module_start: the cycle thread could not be created */

typedef struct SyncthingWorkerLink {
    uint32_t magic;          /* STP_LINK_MAGIC */
    uint32_t version;        /* STP_LINK_VERSION */
    volatile uint32_t abort; /* launcher -> worker: non-zero asks the cycle to stop */
    volatile uint32_t state; /* worker -> launcher: STP_STATE_* */
    volatile int32_t result; /* worker -> launcher: STP_RESULT_* */
} SyncthingWorkerLink;

#endif /* SYNCTHING_PLUGIN_H */
