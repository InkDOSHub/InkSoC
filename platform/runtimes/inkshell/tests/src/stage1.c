// Checks for the Linux layer: threads + futex, signal handlers, local sockets, epoll, eventfd, select.
// Built static: gcc -O1 -static -pthread -o tests/stage1 tests/src/stage1.c
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/epoll.h>
#include <sys/eventfd.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <sys/time.h>
#include <sys/timerfd.h>
#include <time.h>

static pthread_mutex_t mu = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t cv = PTHREAD_COND_INITIALIZER;
static long total; static int ready;
static void *worker(void *a) {
  for (int i = 0; i < 20000; i++) { pthread_mutex_lock(&mu); total += (long)a; pthread_mutex_unlock(&mu); }
  pthread_mutex_lock(&mu); ready++; pthread_cond_signal(&cv); pthread_mutex_unlock(&mu);
  return 0;
}
static volatile int got;
static void on_sig(int s) { got = s; }
static volatile int ticks;
static void on_tick(int s) { ticks++; }

int main(int argc, char **argv) {
  const char *only = argc > 1 ? argv[1] : "all";
  if (!strcmp(only, "all") || !strcmp(only, "threads")) {
    pthread_t t[4];
    for (long i = 0; i < 4; i++) pthread_create(&t[i], 0, worker, (void *)(i + 1));
    pthread_mutex_lock(&mu); while (ready < 4) pthread_cond_wait(&cv, &mu); pthread_mutex_unlock(&mu);
    for (int i = 0; i < 4; i++) pthread_join(t[i], 0);
    printf("threads %ld\n", total); // 20000 * (1+2+3+4) = 200000
  }
  if (!strcmp(only, "all") || !strcmp(only, "signals")) {
    struct sigaction sa = {0}; sa.sa_handler = on_sig; sigaction(SIGUSR1, &sa, 0);
    raise(SIGUSR1);
    int a = got; got = 0;
    pid_t c = fork();
    if (c == 0) { pause(); _exit(got == SIGUSR1 ? 7 : 1); }
    usleep(20000); kill(c, SIGUSR1);
    int st; waitpid(c, &st, 0);
    pid_t d = fork();
    if (d == 0) { for (;;) pause(); }
    kill(d, SIGTERM); int st2; waitpid(d, &st2, 0);
    printf("signals %d %d %d\n", a, WEXITSTATUS(st), WIFSIGNALED(st2) ? WTERMSIG(st2) : 0); // 10 7 15
  }
  if (!strcmp(only, "all") || !strcmp(only, "sockets")) {
    int sv[2]; socketpair(AF_UNIX, SOCK_STREAM, 0, sv);
    write(sv[0], "ping", 4); char b[16] = {0}; read(sv[1], b, sizeof b);
    // a unix server in a child process
    unlink("/tmp/s1.sock");
    int ls = socket(AF_UNIX, SOCK_STREAM, 0);
    struct sockaddr_un ua = {0}; ua.sun_family = AF_UNIX; strcpy(ua.sun_path, "/tmp/s1.sock");
    bind(ls, (void *)&ua, sizeof ua); listen(ls, 4);
    if (fork() == 0) { int c = accept(ls, 0, 0); char q[16] = {0}; read(c, q, 16); write(c, "pong", 4); _exit(0); }
    int cs = socket(AF_UNIX, SOCK_STREAM, 0); connect(cs, (void *)&ua, sizeof ua);
    write(cs, "hi", 2); char r[16] = {0}; read(cs, r, sizeof r); wait(0);
    // TCP on 127.0.0.1
    int ts = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in ia = {0}; ia.sin_family = AF_INET; ia.sin_port = htons(8099); ia.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    bind(ts, (void *)&ia, sizeof ia); listen(ts, 4);
    if (fork() == 0) { int c = accept(ts, 0, 0); write(c, "tcp-ok", 6); close(c); _exit(0); }
    int tc = socket(AF_INET, SOCK_STREAM, 0); int e = connect(tc, (void *)&ia, sizeof ia);
    char tr[16] = {0}; int n = 0, k; while ((k = read(tc, tr + n, sizeof tr - 1 - n)) > 0) n += k; wait(0);
    printf("sockets %s %s %d %s\n", b, r, e, tr); // ping pong 0 tcp-ok
  }
  if (!strcmp(only, "all") || !strcmp(only, "epoll")) {
    int ep = epoll_create1(0), ev = eventfd(0, 0), p[2]; pipe(p);
    struct epoll_event x = {.events = EPOLLIN, .data.u64 = 11}; epoll_ctl(ep, EPOLL_CTL_ADD, ev, &x);
    struct epoll_event y = {.events = EPOLLIN, .data.u64 = 22}; epoll_ctl(ep, EPOLL_CTL_ADD, p[0], &y);
    struct epoll_event out[4];
    int n0 = epoll_wait(ep, out, 4, 0);
    if (fork() == 0) { usleep(20000); write(p[1], "x", 1); _exit(0); }
    int n1 = epoll_wait(ep, out, 4, 2000); long d1 = n1 > 0 ? (long)out[0].data.u64 : -1;
    uint64_t one = 5; write(ev, &one, 8);
    char c; read(p[0], &c, 1);
    int n2 = epoll_wait(ep, out, 4, 0); long d2 = n2 > 0 ? (long)out[0].data.u64 : -1;
    uint64_t v = 0; read(ev, &v, 8); wait(0);
    fd_set rs; FD_ZERO(&rs); FD_SET(p[0], &rs); struct timeval tv = {0, 30000};
    int s0 = select(p[0] + 1, &rs, 0, 0, &tv);
    write(p[1], "y", 1); FD_ZERO(&rs); FD_SET(p[0], &rs); tv.tv_usec = 30000;
    int s1 = select(p[0] + 1, &rs, 0, 0, &tv);
    printf("epoll %d %d %ld %d %ld %lu select %d %d\n", n0, n1, d1, n2, d2, (unsigned long)v, s0, s1); // 0 1 22 1 11 5 select 0 1
  }
  if (!strcmp(only, "all") || !strcmp(only, "pty")) {
    int m = posix_openpt(O_RDWR | O_NOCTTY); grantpt(m); unlockpt(m);
    char *sn = ptsname(m);
    struct winsize ws = {24, 80, 0, 0}; ioctl(m, TIOCSWINSZ, &ws);
    pid_t c = fork();
    if (c == 0) {
      setsid(); int s = open(sn, O_RDWR); ioctl(s, TIOCSCTTY, 0);
      dup2(s, 0); dup2(s, 1); dup2(s, 2); if (s > 2) close(s);
      execl("/bin/sh", "sh", "-c", "stty size; tty >/dev/null && echo is-tty; read x; echo got-$x; exec cat", (char *)0);
      _exit(1);
    }
    write(m, "abc\r", 4);
    char all[4096] = {0}; int n = 0, k, sent = 0;
    while (n < (int)sizeof all - 1 && (k = read(m, all + n, sizeof all - 1 - n)) > 0) {
      n += k; all[n] = 0;
      if (!sent && strstr(all, "got-abc")) { usleep(50000); write(m, "\x03", 1); sent = 1; }
    }
    int st; waitpid(c, &st, 0);
    printf("pty %s %s %s %s sig %d\n", strstr(all, "24 80") ? "size" : "-", strstr(all, "is-tty") ? "tty" : "-",
           strstr(all, "got-abc\r\n") ? "line" : "-", strstr(all, "^C") ? "^C" : "-", WIFSIGNALED(st) ? WTERMSIG(st) : -1);
  }
  if (!strcmp(only, "all") || !strcmp(only, "timers")) {
    signal(SIGALRM, on_sig); got = 0;
    alarm(1); pause();                          // alarm wakes pause with SIGALRM
    int a = got;
    signal(SIGALRM, on_tick);
    struct itimerval it = {{0, 50000}, {0, 50000}};
    setitimer(ITIMER_REAL, &it, 0);
    while (ticks < 3) pause();                  // an interval timer ticks again and again
    struct itimerval off = {0}; setitimer(ITIMER_REAL, &off, 0);
    int tf = timerfd_create(CLOCK_MONOTONIC, 0);
    struct itimerspec ts = {{0, 20000000}, {0, 20000000}};
    timerfd_settime(tf, 0, &ts, 0);
    unsigned long long n = 0, sum = 0;
    while (sum < 3) { read(tf, &n, 8); sum += n; }
    timer_t t; struct sigevent ev = {0}; ev.sigev_notify = SIGEV_SIGNAL; ev.sigev_signo = SIGUSR1;
    signal(SIGUSR1, on_sig); got = 0;
    timer_create(CLOCK_MONOTONIC, &ev, &t);
    struct itimerspec one = {{0, 0}, {0, 30000000}};
    timer_settime(t, 0, &one, 0);
    while (!got) pause();
    pid_t c = fork();
    if (c == 0) { signal(SIGALRM, SIG_DFL); alarm(1); for (;;) usleep(10000); }
    int st; waitpid(c, &st, 0);
    printf("timers %d %d %s %d %d\n", a, ticks >= 3, sum >= 3 ? "fd" : "-", got, WIFSIGNALED(st) ? WTERMSIG(st) : -1);
  }
  return 0;
}
