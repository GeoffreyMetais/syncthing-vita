/* System queries of the launcher (SceAppMgr, SceNetCtl, SceIo, SceThreadMgr). */
#ifndef STP_SYSTEM_H
#define STP_SYSTEM_H

#include <stdint.h>

#define STP_PATH_MAX 64
/* Running applications examined per poll (the PS Vita runs a handful at most). */
#define STP_MAX_APPS 16
/* Title ID of the app: its presence blocks cycles like any app, but is not a "game exit". */
#define STP_GUI_TITLE_ID "SYNCTHING"

/* Monotonic seconds since boot. */
uint32_t stp_now_secs(void);

/* Reads up to `size - 1` bytes of `path`. Returns the length, or -1 if it cannot be read. */
int stp_read_file(const char *path, char *buf, int size);

/* `*app_running`: an app other than SYNCTHING runs (also when the list cannot be read).
 * `*gui_running`: SYNCTHING runs (in the list, or its SyncthingGuiRunning mutex exists). */
void stp_observe_apps(int *app_running, int *gui_running);

/* Title IDs of the running apps, comma-separated, in `out`. Returns the count, or < 0. */
int stp_running_titles(char *out, int size);

/* 1 if Wi-Fi is connected. */
int stp_network_connected(void);

/* Path of syncthing_worker.suprx: in the active tai directory (ux0:tai/ when
 * ux0:tai/config.txt exists, as taiHEN chooses, else ur0:tai/), else in the other one.
 * Returns 0, or -1 if neither holds it. */
int stp_find_worker(char *out, int size);

#endif /* STP_SYSTEM_H */
