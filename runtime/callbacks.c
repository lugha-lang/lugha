/*
 * callbacks.c — closures handed to C. A closure passed where C takes user
 * data is copied into a small record that holds its environment, so it and
 * its captures live while C may call it: until the program exits, or until
 * C calls lugha_rt_cb_release.
 */

/* A copy of the function value { code, env } that C can hold. */
void *lugha_rt_cb_pin(void *code, void *env) {
    void **pinned = lugha_rc_internal(2 * sizeof(void *));
    pinned[0] = code;
    pinned[1] = env;
    lugha_rt_rc_retain(env);
    return pinned;
}

/* C dropped the callback: its closure's environment is let go of. */
void lugha_rt_cb_release(void *pinned) {
    void **record = pinned;
    lugha_rt_rc_release(record[1]);
    free(record);
}
