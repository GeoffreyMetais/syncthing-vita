/*
 * Pure decision logic of the launcher: no system call, so it is compiled and tested on the
 * host (crates/syncthing-vita-worker/tests/launcher_policy.rs builds tests/policy_test.c).
 */
#ifndef STP_POLICY_H
#define STP_POLICY_H

#include <stddef.h>
#include <stdint.h>

/* Defaults of plugin.cfg keys (the app's PLUGIN_DEFAULT_*). */
#define STP_DEFAULT_ENABLED 1
#define STP_DEFAULT_SYNC_ON_GAME_EXIT 1
#define STP_DEFAULT_INTERVAL_SECS 1800u

/* A periodic interval shorter than this is raised to it (interval_secs=0 disables it). */
#define STP_MIN_INTERVAL_SECS 120u
/* Quiet time on the LiveArea after a game exits before the exit cycle starts, so that a game
 * started right after another one never sees the worker. */
#define STP_EXIT_SETTLE_SECS 15u
/* Delay before retrying when a cycle could not start (no Wi-Fi, worker missing, load error). */
#define STP_RETRY_SECS 300u

typedef struct StpSettings {
    int enabled;
    int sync_on_game_exit;
    uint32_t interval_secs;
} StpSettings;

/* What the launcher observed on one poll. */
typedef struct StpObservation {
    uint32_t now_secs;  /* monotonic seconds */
    int app_running;    /* a game or app other than SYNCTHING runs (or the list is unknown) */
    int gui_running;    /* the app (SYNCTHING) runs: title in the list or its mutex exists */
} StpObservation;

typedef enum StpDecision {
    STP_IDLE = 0,
    STP_START_EXIT_CYCLE,     /* a game exited and the LiveArea stayed idle */
    STP_START_PERIODIC_CYCLE, /* interval_secs elapsed with nothing running */
} StpDecision;

typedef struct StpPolicy {
    uint32_t last_cycle_secs; /* end of the last cycle (or of the app's session, or boot) */
    uint32_t retry_after_secs;
    int was_blocked;          /* something ran at the previous poll */
    int app_seen;             /* a game / app (not only SYNCTHING) ran since the last cycle */
    int exit_pending;
    uint32_t idle_since_secs;
    int disabled_until_reboot;
} StpPolicy;

/* Parses plugin.cfg (same rules as the worker's plugin_cfg.rs). */
void stp_parse_settings(const char *text, size_t len, StpSettings *out);

/* Interval actually used: 0 stays 0 (periodic cycles off), others at least the minimum. */
uint32_t stp_effective_interval(uint32_t interval_secs);

void stp_policy_init(StpPolicy *p, uint32_t boot_secs);

/* One poll: returns whether a cycle must start now. `settings` is NULL without plugin.cfg. */
StpDecision stp_policy_tick(StpPolicy *p, const StpSettings *settings, const StpObservation *obs);

/* The cycle ended (or could not start: `failed`) at `now_secs`. */
void stp_policy_cycle_ended(StpPolicy *p, uint32_t now_secs, int failed);

/* A worker thread died: no cycle until the next boot. */
void stp_policy_disable(StpPolicy *p);

/* Whether a running cycle must be aborted. */
int stp_must_abort(const StpSettings *settings, const StpObservation *obs);

#endif /* STP_POLICY_H */
