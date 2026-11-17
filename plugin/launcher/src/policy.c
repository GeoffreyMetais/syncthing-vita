/*
 * Pure decision logic of the launcher (see policy.h). No libc: the launcher is linked
 * without one, so this file only uses plain C.
 */
#include "policy.h"

static int is_space(char c)
{
    return c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == '\v' || c == '\f';
}

/* Trims [*start, *end) in place. */
static void trim(const char **start, const char **end)
{
    while (*start < *end && is_space(**start))
        (*start)++;
    while (*end > *start && is_space((*end)[-1]))
        (*end)--;
}

static int equals(const char *start, const char *end, const char *word)
{
    while (start < end && *word && *start == *word) {
        start++;
        word++;
    }
    return start == end && *word == '\0';
}

/* "0" / "1" -> 0 / 1, anything else -> -1. */
static int parse_flag(const char *start, const char *end)
{
    if (end - start != 1)
        return -1;
    if (*start == '0')
        return 0;
    if (*start == '1')
        return 1;
    return -1;
}

/* Decimal u32 with an optional '+' (as Rust's u32::from_str). Returns 0 if invalid. */
static int parse_u32(const char *start, const char *end, uint32_t *out)
{
    uint32_t value = 0;
    if (start < end && *start == '+')
        start++;
    if (start == end)
        return 0;
    for (; start < end; start++) {
        uint32_t digit;
        if (*start < '0' || *start > '9')
            return 0;
        digit = (uint32_t)(*start - '0');
        if (value > (UINT32_MAX - digit) / 10u)
            return 0;
        value = value * 10u + digit;
    }
    *out = value;
    return 1;
}

static void apply_line(const char *start, const char *end, StpSettings *out)
{
    const char *eq;
    const char *key_end;
    const char *value;
    int flag;
    trim(&start, &end);
    eq = start;
    while (eq < end && *eq != '=')
        eq++;
    if (eq >= end)
        return;
    key_end = eq;
    value = eq + 1;
    trim(&start, &key_end);
    trim(&value, &end);
    if (equals(start, key_end, "enabled")) {
        flag = parse_flag(value, end);
        if (flag >= 0)
            out->enabled = flag;
    } else if (equals(start, key_end, "sync_on_game_exit")) {
        flag = parse_flag(value, end);
        if (flag >= 0)
            out->sync_on_game_exit = flag;
    } else if (equals(start, key_end, "interval_secs")) {
        uint32_t secs;
        if (parse_u32(value, end, &secs))
            out->interval_secs = secs;
    }
}

void stp_parse_settings(const char *text, size_t len, StpSettings *out)
{
    const char *end = text + len;
    const char *line = text;
    out->enabled = STP_DEFAULT_ENABLED;
    out->sync_on_game_exit = STP_DEFAULT_SYNC_ON_GAME_EXIT;
    out->interval_secs = STP_DEFAULT_INTERVAL_SECS;
    while (line < end) {
        const char *eol = line;
        while (eol < end && *eol != '\n')
            eol++;
        apply_line(line, eol, out);
        line = eol + 1;
    }
}

uint32_t stp_effective_interval(uint32_t interval_secs)
{
    if (interval_secs == 0)
        return 0;
    return interval_secs < STP_MIN_INTERVAL_SECS ? STP_MIN_INTERVAL_SECS : interval_secs;
}

void stp_policy_init(StpPolicy *p, uint32_t boot_secs)
{
    p->last_cycle_secs = boot_secs;
    p->retry_after_secs = boot_secs;
    p->was_blocked = 0;
    p->app_seen = 0;
    p->exit_pending = 0;
    p->idle_since_secs = boot_secs;
    p->disabled_until_reboot = 0;
}

StpDecision stp_policy_tick(StpPolicy *p, const StpSettings *settings, const StpObservation *obs)
{
    uint32_t interval;
    if (p->disabled_until_reboot)
        return STP_IDLE;
    if (obs->app_running || obs->gui_running) {
        if (obs->app_running)
            p->app_seen = 1;
        p->was_blocked = 1;
        p->exit_pending = 0;
        return STP_IDLE;
    }
    if (p->was_blocked) {
        p->was_blocked = 0;
        p->idle_since_secs = obs->now_secs;
        if (p->app_seen)
            p->exit_pending = 1;
        else
            p->last_cycle_secs = obs->now_secs; /* only the app ran: it just synchronized */
        p->app_seen = 0;
    }
    if (settings == 0 || !settings->enabled) {
        p->exit_pending = 0;
        return STP_IDLE;
    }
    if (!settings->sync_on_game_exit)
        p->exit_pending = 0;
    if (obs->now_secs < p->retry_after_secs
        || obs->now_secs - p->idle_since_secs < STP_EXIT_SETTLE_SECS)
        return STP_IDLE;
    if (p->exit_pending) {
        p->exit_pending = 0;
        return STP_START_EXIT_CYCLE;
    }
    interval = stp_effective_interval(settings->interval_secs);
    if (interval != 0 && obs->now_secs - p->last_cycle_secs >= interval)
        return STP_START_PERIODIC_CYCLE;
    return STP_IDLE;
}

void stp_policy_cycle_ended(StpPolicy *p, uint32_t now_secs, int failed)
{
    p->exit_pending = 0;
    if (failed) {
        p->retry_after_secs = now_secs + STP_RETRY_SECS;
    } else {
        p->last_cycle_secs = now_secs;
        p->retry_after_secs = now_secs;
    }
}

void stp_policy_disable(StpPolicy *p)
{
    p->disabled_until_reboot = 1;
}

int stp_must_abort(const StpSettings *settings, const StpObservation *obs)
{
    return obs->app_running || obs->gui_running || settings == 0 || !settings->enabled;
}
