/* The launcher's lines in syncthing.log (see log.h). */
#include <psp2/io/fcntl.h>
#include <psp2/io/stat.h>
#include <psp2/kernel/clib.h>

#include "../../include/syncthing_plugin.h"
#include "log.h"
#include "system.h"

/* The app's LOG_MAX_BYTES: above it the log becomes syncthing.log.1. */
#define LOG_MAX_BYTES (512 * 1024)
#define LOG_PREFIX "[VITA-SYNCTHING] Background sync: "
#define LOG_LINE_MAX 256

void stp_log(const char *msg)
{
    char line[LOG_LINE_MAX];
    SceIoStat st;
    int len = sceClibSnprintf(line, sizeof(line), LOG_PREFIX "%s\n", msg);
    SceUID fd;
    if (len <= 0)
        return;
    if (len >= (int)sizeof(line)) {
        len = (int)sizeof(line) - 1;
        line[len - 1] = '\n';
    }
    fd = sceIoOpen(STP_LOG_FILE, SCE_O_WRONLY | SCE_O_CREAT | SCE_O_APPEND, 0666);
    if (fd < 0)
        return;
    sceIoWrite(fd, line, (SceSize)len);
    sceIoClose(fd);
    if (sceIoGetstat(STP_LOG_FILE, &st) >= 0 && st.st_size > LOG_MAX_BYTES) {
        sceIoRemove(STP_ROTATED_LOG_FILE);
        sceIoRename(STP_LOG_FILE, STP_ROTATED_LOG_FILE);
    }
}

void stp_log_running_apps(const char *what)
{
    char titles[160];
    char msg[200];
    int count = stp_running_titles(titles, sizeof(titles));
    if (count < 0)
        sceClibSnprintf(msg, sizeof(msg), "%s (running apps unknown: 0x%08X)", what,
                        (unsigned)count);
    else
        sceClibSnprintf(msg, sizeof(msg), "%s (%d running app(s)%s%s)", what, count,
                        count ? ": " : "", titles);
    stp_log(msg);
}
