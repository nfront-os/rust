/*
 * APE's read buffering, select and poll, for the C in a Rust program on 9front
 * (in place of APE's _buf.c, which libape9-ap leaves out).
 *
 * Plan 9 cannot ask a descriptor whether a read would block. APE reads such a
 * descriptor in a proc of its own into a buffer, which read(2) then takes from
 * (_readbuf); select waits for those procs. APE's way does not work here: it forks the
 * copy procs on the stack it runs on, which in a Rust program is shared memory (the stack
 * slots of std, also without RFMEM), attaches its buffers' segment when first needed (the
 * program's other procs do not have it), and lets one proc wait at a time.
 *
 * Here the copy procs are forked RFMEM|RFFDG through std (__plan9_rforkproc: on their own
 * stack segment, ended with the program), the buffers are from malloc, and any number of
 * procs wait in poll or select, each on a semaphore of its own: the copy procs wake them
 * all when data or the end comes.
 */
/* as APE's own (sys/src/ape/lib/ap/plan9/mkfile) */
#define _POSIX_SOURCE
#define _PLAN9_SOURCE
#define _BSD_EXTENSION
#define _LOCK_EXTENSION
#define _RESEARCH_SOURCE
#include <sys/types.h>
#include <stdio.h>
#include "lib.h"
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
#include <time.h>
#include <lock.h>
#include <sys/time.h>
#include <sys/select.h>
#include <poll.h>
#include "sys9.h"
#include "dir.h"

extern int _SEMACQUIRE(int *, int);
extern int _TSEMACQUIRE(int *, unsigned long);
extern int _SEMRELEASE(int *, int);
/* std's (sys/pal/plan9/proc.rs) */
extern int __plan9_rforkproc(int, void (*)(void *), void *);

enum {
	RFFDG_ = 1<<2,
	RFMEM_ = 1<<5,
	RFNOWAIT_ = 1<<6,
};

typedef struct Buf Buf;
struct Buf {
	int	fd;
	int	pid;		/* of the copy proc */
	int	closed;		/* the program closed fd */
	int	eof;		/* after the data */
	int	r, w;		/* data[r..w] unread */
	int	roomwait;	/* the copy proc waits for room */
	int	room;		/* its semaphore */
	int	datawait;	/* a reader waits for data */
	int	data_;		/* its semaphore */
	unsigned char	data[PERFDMAX];
};

typedef struct Waiter Waiter;
struct Waiter {
	int	sem;
	Waiter	*next;
};

static Lock lk;
static Waiter *waiters;

/* libbsd's listen.c knows these */
int _muxsid = -1;

void
_killmuxsid(void)
{
}

static void
semwait(int *s)
{
	while(_SEMACQUIRE(s, 1) < 0)
		;
}

/* called with lk: the data or the end came */
static void
wakeall(Buf *b)
{
	Waiter *w;

	if(b->datawait){
		b->datawait = 0;
		_SEMRELEASE(&b->data_, 1);
	}
	for(w = waiters; w != NULL; w = w->next)
		_SEMRELEASE(&w->sem, 1);
	waiters = NULL;
}

/*
 * The copy proc has a copy of the descriptors (RFFDG), which the program's procs share:
 * all but its own go, as in APE, or it would hold others' connections open. dup(3)'s #d
 * lists them; malloc is fine (RFMEM: the heap and its lock are the program's).
 */
static void
closeothers(int keep)
{
	Dir *d;
	char *p;
	int dfd, fd, i, n;

	/* raw calls: APE's _fdinfo is the program's (RFMEM), not to be touched here */
	dfd = _OPEN("#d", 0);
	if(dfd < 0)
		return;
	n = _dirreadall(dfd, &d);
	for(i = 0; i < n; i++){
		fd = strtol(d[i].name, &p, 10);
		if(*p != '\0' || p == d[i].name || fd == keep || fd == dfd)
			continue;
		_CLOSE(fd);
	}
	_CLOSE(dfd);
	if(n > 0)
		free(d);
}

static void
copyproc(void *v)
{
	Buf *b;
	int n, nzeros;

	b = v;
	closeothers(b->fd);
	for(;;){
		lock(&lk);
		if(b->closed){
			unlock(&lk);
			return;
		}
		if(b->r == b->w)
			b->r = b->w = 0;
		if(PERFDMAX - b->w < READMAX){
			b->roomwait = 1;
			unlock(&lk);
			semwait(&b->room);
			continue;
		}
		unlock(&lk);
		/*
		 * A zero-length read might be a zero-length write or the end:
		 * several, as APE (POSIX read() drops zero-length messages)
		 */
		nzeros = 0;
		do
			n = _READ(b->fd, b->data + b->w, READMAX);
		while(n == 0 && ++nzeros < 3 && !b->closed);
		lock(&lk);
		if(b->closed){
			unlock(&lk);
			return;
		}
		if(n <= 0)
			b->eof = 1;
		else
			b->w += n;
		wakeall(b);
		unlock(&lk);
		if(n <= 0)
			return;
	}
}

/* fd read through a copy proc from now on; -1 if it cannot be */
int
_startbuf(int fd)
{
	Fdinfo *f;
	Buf *b;
	int pid;

	if(fd < 0)
		return 0;
	if(fd >= OPEN_MAX){
		errno = EBADF;
		return -1;
	}
	f = &_fdinfo[fd];
	lock(&lk);
	if((f->flags&FD_ISOPEN) == 0){
		unlock(&lk);
		errno = EBADF;
		return -1;
	}
	if(f->flags&FD_BUFFERED){
		unlock(&lk);
		return 0;
	}
	b = calloc(1, sizeof *b);
	if(b == NULL){
		unlock(&lk);
		errno = ENOMEM;
		return -1;
	}
	b->fd = fd;
	f->buf = (Muxbuf *)b;
	f->flags |= FD_BUFFERED;
	unlock(&lk);
	pid = __plan9_rforkproc(RFMEM_|RFFDG_|RFNOWAIT_, copyproc, b);
	if(pid < 0){
		lock(&lk);
		f->flags &= ~FD_BUFFERED;
		f->buf = NULL;
		unlock(&lk);
		free(b);
		_syserrno();
		return -1;
	}
	b->pid = pid;
	return 0;
}

/* fd is closed: its copy proc goes */
void
_closebuf(int fd)
{
	Buf *b;
	char path[32];
	int cfd, done;

	b = (Buf *)_fdinfo[fd].buf;
	if(b == NULL)
		return;
	lock(&lk);
	b->closed = 1;
	done = b->eof;
	if(b->roomwait){
		b->roomwait = 0;
		_SEMRELEASE(&b->room, 1);
	}
	wakeall(b);
	unlock(&lk);
	_fdinfo[fd].buf = NULL;
	if(done){
		/* the copy proc has returned or returns without touching b */
		free(b);
		return;
	}
	/* a copy proc in a read: proc(3)'s kill; its buffer stays (it might still write) */
	snprintf(path, sizeof path, "/proc/%d/ctl", b->pid);
	cfd = _OPEN(path, 1);
	if(cfd >= 0){
		_WRITE(cfd, "kill", 4);
		_CLOSE(cfd);
	}
}

/* read(2) of a buffered fd; noblock: none if there is no data */
int
_readbuf(int fd, void *addr, int nwant, int noblock)
{
	Buf *b;
	int n;

	b = (Buf *)_fdinfo[fd].buf;
	if(b == NULL || b->fd != fd){
		errno = EBADF;
		return -1;
	}
	lock(&lk);
	for(;;){
		n = b->w - b->r;
		if(n > 0 || b->eof || b->closed)
			break;
		if(noblock){
			unlock(&lk);
			errno = EAGAIN;
			return -1;
		}
		b->datawait = 1;
		unlock(&lk);
		semwait(&b->data_);
		lock(&lk);
	}
	if(n > nwant)
		n = nwant;
	memcpy(addr, b->data + b->r, n);
	b->r += n;
	if(b->r == b->w && b->roomwait){
		b->r = b->w = 0;
		b->roomwait = 0;
		_SEMRELEASE(&b->room, 1);
	}
	unlock(&lk);
	return n;
}

/* a forked child cannot read its parent's copy procs' buffers */
void
_detachbuf(void)
{
	int i;

	for(i = 0; i < OPEN_MAX; i++)
		if(_fdinfo[i].flags&FD_BUFFERED){
			_fdinfo[i].flags &= ~FD_BUFFERED;
			_fdinfo[i].buf = NULL;
		}
}

static long long
msnow(void)
{
	struct timespec ts;

	clock_gettime(CLOCK_MONOTONIC, &ts);
	return (long long)ts.tv_sec*1000 + ts.tv_nsec/1000000;
}

/* called with lk */
static int
ready(struct pollfd *fds, nfds_t n)
{
	nfds_t i;
	int c;
	Fdinfo *f;
	Buf *b;

	c = 0;
	for(i = 0; i < n; i++){
		fds[i].revents = 0;
		if(fds[i].fd < 0)
			continue;
		if(fds[i].fd >= OPEN_MAX || (_fdinfo[fds[i].fd].flags&FD_ISOPEN) == 0){
			fds[i].revents = POLLNVAL;
			c++;
			continue;
		}
		f = &_fdinfo[fds[i].fd];
		/* writing: assumed ready, as APE's select */
		fds[i].revents |= fds[i].events & (POLLOUT|POLLWRNORM|POLLWRBAND);
		b = (Buf *)f->buf;
		if((f->flags&FD_BUFFERED) && b != NULL){
			if(b->w > b->r || b->eof)
				fds[i].revents |= fds[i].events & (POLLIN|POLLRDNORM|POLLRDBAND|POLLPRI);
			if(b->eof && b->w == b->r)
				fds[i].revents |= POLLHUP;
		}
		if(fds[i].revents)
			c++;
	}
	return c;
}

int
poll(struct pollfd *fds, nfds_t n, int ms)
{
	nfds_t i;
	Waiter w, **l;
	long long deadline, left;
	int c, r;

	for(i = 0; i < n; i++)
		if(fds[i].fd >= 0 && fds[i].fd < OPEN_MAX
		&& (fds[i].events & (POLLIN|POLLRDNORM|POLLRDBAND|POLLPRI))
		&& (_fdinfo[fds[i].fd].flags&(FD_ISOPEN|FD_BUFFERED)) == FD_ISOPEN)
			if(_startbuf(fds[i].fd) < 0)
				return -1;
	deadline = ms > 0 ? msnow() + ms : 0;
	for(;;){
		lock(&lk);
		c = ready(fds, n);
		if(c > 0 || ms == 0){
			unlock(&lk);
			return c;
		}
		w.sem = 0;
		w.next = waiters;
		waiters = &w;
		unlock(&lk);
		if(ms < 0)
			r = _SEMACQUIRE(&w.sem, 1);
		else{
			left = deadline - msnow();
			r = left > 0 ? _TSEMACQUIRE(&w.sem, left) : 0;
		}
		lock(&lk);
		for(l = &waiters; *l != NULL; l = &(*l)->next)
			if(*l == &w){
				*l = w.next;
				break;
			}
		unlock(&lk);
		if(r < 0){
			errno = EINTR;
			return -1;
		}
		if(r == 0 && ms > 0 && msnow() >= deadline){
			lock(&lk);
			c = ready(fds, n);
			unlock(&lk);
			return c;
		}
	}
}

int
select(int nfds, fd_set *rfds, fd_set *wfds, fd_set *efds, struct timeval *tv)
{
	struct pollfd p[FD_SETSIZE];
	int i, n, c, ms;

	if(nfds > FD_SETSIZE)
		nfds = FD_SETSIZE;
	n = 0;
	for(i = 0; i < nfds; i++){
		p[n].fd = i;
		p[n].events = 0;
		if(rfds && FD_ISSET(i, rfds))
			p[n].events |= POLLIN;
		if(wfds && FD_ISSET(i, wfds))
			p[n].events |= POLLOUT;
		if(efds && FD_ISSET(i, efds))
			p[n].events |= POLLPRI;
		if(p[n].events)
			n++;
	}
	ms = tv == NULL ? -1 : tv->tv_sec*1000 + (tv->tv_usec+999)/1000;
	c = poll(p, n, ms);
	if(c < 0)
		return -1;
	if(rfds)
		FD_ZERO(rfds);
	if(wfds)
		FD_ZERO(wfds);
	if(efds)
		FD_ZERO(efds);
	c = 0;
	for(i = 0; i < n; i++){
		if(rfds && (p[i].revents & (POLLIN|POLLHUP)))
			FD_SET(p[i].fd, rfds), c++;
		if(wfds && (p[i].revents & POLLOUT))
			FD_SET(p[i].fd, wfds), c++;
		if(efds && (p[i].revents & (POLLERR|POLLNVAL)))
			FD_SET(p[i].fd, efds), c++;
	}
	return c;
}
