/* chanping: two threads bounce a counter through one-slot handoffs, n
 * round trips; a mutex and condition variable stand in for channels. */
#include <pthread.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    pthread_mutex_t lock;
    pthread_cond_t changed;
    long value;
    bool full;
    bool closed;
} Slot;

static Slot ping = {PTHREAD_MUTEX_INITIALIZER, PTHREAD_COND_INITIALIZER, 0, false, false};
static Slot pong = {PTHREAD_MUTEX_INITIALIZER, PTHREAD_COND_INITIALIZER, 0, false, false};

static void put(Slot *s, long v) {
    pthread_mutex_lock(&s->lock);
    while (s->full) {
        pthread_cond_wait(&s->changed, &s->lock);
    }
    s->value = v;
    s->full = true;
    pthread_cond_broadcast(&s->changed);
    pthread_mutex_unlock(&s->lock);
}

/* false once the slot is closed and empty. */
static bool take(Slot *s, long *v) {
    pthread_mutex_lock(&s->lock);
    while (!s->full && !s->closed) {
        pthread_cond_wait(&s->changed, &s->lock);
    }
    bool got = s->full;
    if (got) {
        *v = s->value;
        s->full = false;
        pthread_cond_broadcast(&s->changed);
    }
    pthread_mutex_unlock(&s->lock);
    return got;
}

static void *echo(void *arg) {
    (void)arg;
    long v;
    while (take(&ping, &v)) {
        put(&pong, v + 1);
    }
    return NULL;
}

int main(int argc, char **argv) {
    long n = argc > 1 ? atol(argv[1]) : 0;
    pthread_t t;
    pthread_create(&t, NULL, echo, NULL);
    long v = 0;
    for (long i = 0; i < n; i++) {
        put(&ping, v);
        take(&pong, &v);
    }
    pthread_mutex_lock(&ping.lock);
    ping.closed = true;
    pthread_cond_broadcast(&ping.changed);
    pthread_mutex_unlock(&ping.lock);
    pthread_join(t, NULL);
    printf("%ld\n", v);
    return 0;
}
