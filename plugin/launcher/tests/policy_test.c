/*
 * Host test of the launcher's decision logic (src/policy.c), built and run by
 * crates/syncthing-vita-worker/tests/launcher_policy.rs with the host C compiler.
 * Prints one line per failed check and exits non-zero if any failed.
 */
#include <stdio.h>
#include <string.h>

#include "../src/policy.h"

static int failures = 0;

#define CHECK(cond)                                                             \
    do {                                                                        \
        if (!(cond)) {                                                          \
            printf("FAILED %s:%d: %s\n", __FILE__, __LINE__, #cond);            \
            failures++;                                                         \
        }                                                                       \
    } while (0)

static StpSettings parse(const char *text)
{
    StpSettings s;
    stp_parse_settings(text, strlen(text), &s);
    return s;
}

static StpObservation at(uint32_t now, int app, int gui)
{
    StpObservation o;
    o.now_secs = now;
    o.app_running = app;
    o.gui_running = gui;
    return o;
}

static StpDecision tick(StpPolicy *p, const StpSettings *s, uint32_t now, int app, int gui)
{
    StpObservation o = at(now, app, gui);
    return stp_policy_tick(p, s, &o);
}

static void test_parse(void)
{
    StpSettings s = parse("");
    CHECK(s.enabled == 1 && s.sync_on_game_exit == 1 && s.interval_secs == 1800);
    s = parse("enabled=0\nsync_on_game_exit=0\ninterval_secs=600\n");
    CHECK(s.enabled == 0 && s.sync_on_game_exit == 0 && s.interval_secs == 600);
    s = parse("enabled=1\r\nsync_on_game_exit=1\r\ninterval_secs=0\r\n");
    CHECK(s.enabled == 1 && s.sync_on_game_exit == 1 && s.interval_secs == 0);
    s = parse("  enabled = 0  \ninterval_secs=abc\nsync_on_game_exit=yes\n");
    CHECK(s.enabled == 0 && s.interval_secs == 1800 && s.sync_on_game_exit == 1);
    s = parse("interval_secs=60\ninterval_secs=-5\ninterval_secs=90\nenabled=1\nenabled=0");
    CHECK(s.interval_secs == 90 && s.enabled == 0);
    s = parse("# comment\nunknown=1\nenabled\n=0\ninterval_secs=4294967296\n");
    CHECK(s.enabled == 1 && s.sync_on_game_exit == 1 && s.interval_secs == 1800);
    CHECK(parse("interval_secs=4294967295").interval_secs == 4294967295u);
    CHECK(parse("interval_secs=+7").interval_secs == 7);
    CHECK(parse("interval_secs=+").interval_secs == 1800);
    CHECK(parse("interval_secs=").interval_secs == 1800);
    CHECK(parse("enabled=10").enabled == 1);
    CHECK(parse("enabledx=0").enabled == 1);
    CHECK(parse("xenabled=0").enabled == 1);
    CHECK(parse("enabled=0=1").enabled == 1);
    /* Not NUL-terminated: only `len` bytes are read. */
    {
        StpSettings t;
        stp_parse_settings("enabled=0enabled=1", 9, &t);
        CHECK(t.enabled == 0);
    }
}

static void test_interval(void)
{
    CHECK(stp_effective_interval(0) == 0);
    CHECK(stp_effective_interval(1) == STP_MIN_INTERVAL_SECS);
    CHECK(stp_effective_interval(STP_MIN_INTERVAL_SECS - 1) == STP_MIN_INTERVAL_SECS);
    CHECK(stp_effective_interval(STP_MIN_INTERVAL_SECS) == STP_MIN_INTERVAL_SECS);
    CHECK(stp_effective_interval(STP_MIN_INTERVAL_SECS + 1) == STP_MIN_INTERVAL_SECS + 1);
    CHECK(stp_effective_interval(3600) == 3600);
}

static void test_periodic(void)
{
    StpPolicy p;
    StpSettings s = parse("enabled=1\ninterval_secs=600\n");
    stp_policy_init(&p, 100);
    CHECK(tick(&p, &s, 100, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 699, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 700, 0, 0) == STP_START_PERIODIC_CYCLE);
    stp_policy_cycle_ended(&p, 760, 0);
    CHECK(tick(&p, &s, 1359, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 1360, 0, 0) == STP_START_PERIODIC_CYCLE);
    /* Never while something runs. */
    CHECK(tick(&p, &s, 5000, 1, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 5000, 0, 1) == STP_IDLE);
    /* interval_secs=0: no periodic cycle. */
    s = parse("interval_secs=0\n");
    stp_policy_init(&p, 0);
    CHECK(tick(&p, &s, 100000, 0, 0) == STP_IDLE);
    /* A tiny interval is raised to the minimum. */
    s = parse("interval_secs=1\n");
    stp_policy_init(&p, 0);
    CHECK(tick(&p, &s, STP_MIN_INTERVAL_SECS - 1, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, STP_MIN_INTERVAL_SECS, 0, 0) == STP_START_PERIODIC_CYCLE);
}

static void test_game_exit(void)
{
    StpPolicy p;
    StpSettings s = parse("enabled=1\nsync_on_game_exit=1\ninterval_secs=0\n");
    stp_policy_init(&p, 0);
    CHECK(tick(&p, &s, 10, 1, 0) == STP_IDLE);    /* game running */
    CHECK(tick(&p, &s, 500, 1, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 505, 0, 0) == STP_IDLE);   /* exited: settle first */
    CHECK(tick(&p, &s, 505 + STP_EXIT_SETTLE_SECS - 1, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 505 + STP_EXIT_SETTLE_SECS, 0, 0) == STP_START_EXIT_CYCLE);
    stp_policy_cycle_ended(&p, 600, 0);
    CHECK(tick(&p, &s, 700, 0, 0) == STP_IDLE);   /* only once per exit */

    /* Another game started during the settle time: no cycle until it exits too. */
    stp_policy_init(&p, 0);
    tick(&p, &s, 10, 1, 0);
    CHECK(tick(&p, &s, 20, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 25, 1, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 25 + STP_EXIT_SETTLE_SECS + 5, 1, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 100, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 100 + STP_EXIT_SETTLE_SECS, 0, 0) == STP_START_EXIT_CYCLE);

    /* sync_on_game_exit=0: nothing after a game. */
    s = parse("sync_on_game_exit=0\ninterval_secs=0\n");
    stp_policy_init(&p, 0);
    tick(&p, &s, 10, 1, 0);
    tick(&p, &s, 20, 0, 0);
    CHECK(tick(&p, &s, 20 + STP_EXIT_SETTLE_SECS, 0, 0) == STP_IDLE);

    /* The periodic timer also waits for the settle time after a game. */
    s = parse("sync_on_game_exit=0\ninterval_secs=600\n");
    stp_policy_init(&p, 0);
    tick(&p, &s, 590, 1, 0);
    CHECK(tick(&p, &s, 700, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 700 + STP_EXIT_SETTLE_SECS, 0, 0) == STP_START_PERIODIC_CYCLE);
}

static void test_app_session_counts_as_a_sync(void)
{
    StpPolicy p;
    StpSettings s = parse("enabled=1\nsync_on_game_exit=1\ninterval_secs=600\n");
    stp_policy_init(&p, 0);
    CHECK(tick(&p, &s, 500, 0, 1) == STP_IDLE);   /* the app runs */
    CHECK(tick(&p, &s, 900, 0, 0) == STP_IDLE);   /* closed: no exit cycle */
    CHECK(tick(&p, &s, 900 + STP_EXIT_SETTLE_SECS, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &s, 1499, 0, 0) == STP_IDLE);  /* the interval restarts at its end */
    CHECK(tick(&p, &s, 1500, 0, 0) == STP_START_PERIODIC_CYCLE);

    /* A game and the app: the game's exit still syncs. */
    stp_policy_init(&p, 0);
    tick(&p, &s, 10, 1, 1);
    tick(&p, &s, 20, 0, 0);
    CHECK(tick(&p, &s, 20 + STP_EXIT_SETTLE_SECS, 0, 0) == STP_START_EXIT_CYCLE);
}

static void test_disabled_and_failures(void)
{
    StpPolicy p;
    StpSettings off = parse("enabled=0\ninterval_secs=600\n");
    StpSettings on = parse("enabled=1\ninterval_secs=600\n");
    stp_policy_init(&p, 0);
    CHECK(tick(&p, &off, 10000, 0, 0) == STP_IDLE);
    CHECK(tick(&p, 0, 10000, 0, 0) == STP_IDLE);  /* no plugin.cfg */
    CHECK(tick(&p, &on, 10000, 0, 0) == STP_START_PERIODIC_CYCLE);
    /* A pending exit cycle is dropped while disabled. */
    tick(&p, &on, 10010, 1, 0);
    tick(&p, &off, 10020, 0, 0);
    stp_policy_cycle_ended(&p, 10020, 0);
    CHECK(tick(&p, &on, 10020 + STP_EXIT_SETTLE_SECS, 0, 0) == STP_IDLE);

    /* A cycle that could not start is retried after STP_RETRY_SECS. */
    stp_policy_init(&p, 0);
    CHECK(tick(&p, &on, 600, 0, 0) == STP_START_PERIODIC_CYCLE);
    stp_policy_cycle_ended(&p, 600, 1);
    CHECK(tick(&p, &on, 600 + STP_RETRY_SECS - 1, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &on, 600 + STP_RETRY_SECS, 0, 0) == STP_START_PERIODIC_CYCLE);
    stp_policy_cycle_ended(&p, 1000, 0);
    CHECK(tick(&p, &on, 1599, 0, 0) == STP_IDLE);
    CHECK(tick(&p, &on, 1600, 0, 0) == STP_START_PERIODIC_CYCLE);

    /* A fatal worker error stops everything until reboot. */
    stp_policy_disable(&p);
    CHECK(tick(&p, &on, 100000, 0, 0) == STP_IDLE);
}

static void test_abort(void)
{
    StpSettings on = parse("enabled=1\n");
    StpSettings off = parse("enabled=0\n");
    StpObservation idle = at(0, 0, 0);
    StpObservation game = at(0, 1, 0);
    StpObservation gui = at(0, 0, 1);
    CHECK(!stp_must_abort(&on, &idle));
    CHECK(stp_must_abort(&on, &game));
    CHECK(stp_must_abort(&on, &gui));
    CHECK(stp_must_abort(&off, &idle));
    CHECK(stp_must_abort(0, &idle));
}

int main(void)
{
    test_parse();
    test_interval();
    test_periodic();
    test_game_exit();
    test_app_session_counts_as_a_sync();
    test_disabled_and_failures();
    test_abort();
    if (failures) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("launcher policy: all checks passed\n");
    return 0;
}
