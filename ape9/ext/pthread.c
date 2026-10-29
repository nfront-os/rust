/*
 * Of POSIX threads, the mutexes, for the C in a Rust program on 9front (sqlite): the
 * threads are std's procs, so pthread_self is the pid. 9front-gcc's full pthreads
 * (libapecompat's pthread.o) are left out: they run main themselves, as std does.
 *
 * A mutex is Drepper's three-state futex mutex with a kernel semaphore for the futex, as
 * in 9front-gcc's pthread.c: __val 0 free, 1 locked, 2 locked and maybe waiters.
 * pthread_create cannot make a thread here: EAGAIN (sqlite runs without worker threads).
 */
#include <pthread.h>
#include <errno.h>
#include <stdint.h>
#include <unistd.h>

extern int _SEMACQUIRE(int *, int);
extern int _SEMRELEASE(int *, int);

pthread_t
pthread_self(void)
{
	return (pthread_t)(uintptr_t)getpid();
}

int
pthread_equal(pthread_t a, pthread_t b)
{
	return a == b;
}

int
pthread_create(pthread_t *t, const pthread_attr_t *a, void *(*f)(void *), void *arg)
{
	(void)t; (void)a; (void)f; (void)arg;
	return EAGAIN;
}

int
pthread_join(pthread_t t, void **ret)
{
	(void)t; (void)ret;
	return ESRCH;
}

int
pthread_mutexattr_init(pthread_mutexattr_t *a)
{
	a->__type = PTHREAD_MUTEX_DEFAULT;
	return 0;
}

int
pthread_mutexattr_destroy(pthread_mutexattr_t *a)
{
	(void)a;
	return 0;
}

int
pthread_mutexattr_settype(pthread_mutexattr_t *a, int type)
{
	if(type < PTHREAD_MUTEX_NORMAL || type > PTHREAD_MUTEX_ERRORCHECK)
		return EINVAL;
	a->__type = type;
	return 0;
}

int
pthread_mutex_init(pthread_mutex_t *m, const pthread_mutexattr_t *a)
{
	m->__val = 0;
	m->__sem = 0;
	m->__type = a != NULL ? a->__type : PTHREAD_MUTEX_DEFAULT;
	m->__count = 0;
	m->__owner = NULL;
	return 0;
}

int
pthread_mutex_destroy(pthread_mutex_t *m)
{
	(void)m;
	return 0;
}

static int
owned(pthread_mutex_t *m)
{
	return m->__val != 0 && m->__owner == pthread_self();
}

int
pthread_mutex_trylock(pthread_mutex_t *m)
{
	int z;

	if(m->__type != PTHREAD_MUTEX_NORMAL && owned(m)){
		if(m->__type == PTHREAD_MUTEX_ERRORCHECK)
			return EBUSY;
		m->__count++;
		return 0;
	}
	z = 0;
	if(!__atomic_compare_exchange_n(&m->__val, &z, 1, 0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED))
		return EBUSY;
	m->__owner = pthread_self();
	m->__count = 1;
	return 0;
}

int
pthread_mutex_lock(pthread_mutex_t *m)
{
	int c;

	if(m->__type != PTHREAD_MUTEX_NORMAL && owned(m)){
		if(m->__type == PTHREAD_MUTEX_ERRORCHECK)
			return EDEADLK;
		m->__count++;
		return 0;
	}
	c = 0;
	if(!__atomic_compare_exchange_n(&m->__val, &c, 1, 0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)){
		if(c != 2)
			c = __atomic_exchange_n(&m->__val, 2, __ATOMIC_ACQUIRE);
		while(c != 0){
			/* a note interrupts it: wait on */
			while(_SEMACQUIRE(&m->__sem, 1) < 0)
				;
			c = __atomic_exchange_n(&m->__val, 2, __ATOMIC_ACQUIRE);
		}
	}
	m->__owner = pthread_self();
	m->__count = 1;
	return 0;
}

int
pthread_mutex_unlock(pthread_mutex_t *m)
{
	if(m->__type != PTHREAD_MUTEX_NORMAL){
		if(!owned(m))
			return EPERM;
		if(--m->__count > 0)
			return 0;
	}
	m->__owner = NULL;
	if(__atomic_exchange_n(&m->__val, 0, __ATOMIC_RELEASE) == 2)
		_SEMRELEASE(&m->__sem, 1);
	return 0;
}
