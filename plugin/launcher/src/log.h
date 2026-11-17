/* The launcher's lines in the app's syncthing.log (same prefix and rotation as the app). */
#ifndef STP_LOG_H
#define STP_LOG_H

/* Appends "[VITA-SYNCTHING] Background sync: <msg>" to syncthing.log. */
void stp_log(const char *msg);

/* Logs `what` followed by the title IDs of the running apps (diagnostics of the app check). */
void stp_log_running_apps(const char *what);

#endif /* STP_LOG_H */
