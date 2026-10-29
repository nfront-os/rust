/*
 * APE, Plan 9's POSIX layer, in a native Plan 9 program (Rust's std is on 9front's own
 * libc): libape9-*.a are libap, libbsd and 9front-gcc's libapecompat with every symbol
 * they define renamed to __ape_<name> (scripts/ape9.sh), so that C libraries built
 * against APE (renamed the same, scripts/ape9-ar) do not meet libc's functions of the same
 * names. APE's malloc is left out: malloc, realloc, free are libc's, one heap.
 *
 * APE's _main is not run: this does what it does (callmain.c, _envsetup.c), from
 * .init_array (libgcc's __wrap_main runs it before main), except taking the notes:
 * the program's own handler stays (_envsetup's _NOTIFY is __ape9_nonotify).
 */
#include <stddef.h>

extern void *__ape__tos;
extern int *__ape__errnoloc;
extern char *__ape__plan9err;
extern void __ape__envsetup(void);

/* libc's */
extern void *_tos;
extern void **privalloc(void);
extern void *mallocalign(unsigned int, unsigned int, long, unsigned int);

static char errbuf[128];

int
__ape9_nonotify(void *f)
{
	(void)f;
	return 0;
}

__attribute__((constructor)) void
__ape9_init(void)
{
	/* the top of the stack segment (Tos: pid, clock), APE's main9.S takes it from the kernel */
	__ape__tos = _tos;
	/* errno per proc: a slot of libc's _privates, on the proc's own stack segment */
	__ape__errnoloc = (int *)privalloc();
	/* the error text: one for all procs */
	__ape__plan9err = errbuf;
	__ape__envsetup();
}

/* APE's malloc.o is out: its aligned allocations from libc's */
void *
__ape_memalign(size_t align, size_t size)
{
	return mallocalign(size, align, 0, 0);
}

void *
__ape_aligned_alloc(size_t align, size_t size)
{
	return mallocalign(size, align, 0, 0);
}

int
__ape_posix_memalign(void **p, size_t align, size_t size)
{
	*p = mallocalign(size, align, 0, 0);
	return *p == NULL ? 12 : 0;	/* ENOMEM */
}
