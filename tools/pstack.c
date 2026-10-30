/*
 * pstack pid...: for each proc its pc and sp (/proc/N/regs, amd64 Ureg) and every word
 * on its stack (32K from sp, /proc/N/mem) that points into its text segment
 * (/proc/N/segment): the return addresses among them. acid cannot read the ELF symbols
 * of 9front's gcc port; on the build host addr2line -f -e binary resolves the addresses
 * (tools/pstack-resolve.sh). An APE program, built with 9front-gcc's x86_64-pc-plan9-gcc.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>

typedef unsigned long long u64;

enum {
	PCOFF = 144,	/* offsetof(Ureg, pc): 15 registers, 4 segment selectors, type, error */
	SPOFF = 168,	/* offsetof(Ureg, sp) */
	STACK = 32768,
};

static int
textseg(int pid, u64 *lo, u64 *hi)
{
	char path[64], line[256];
	FILE *f;

	snprintf(path, sizeof path, "/proc/%d/segment", pid);
	f = fopen(path, "r");
	if(f == NULL)
		return -1;
	while(fgets(line, sizeof line, f) != NULL)
		if(strncmp(line, "Text", 4) == 0){
			char *p = line + 4;
			while(*p == ' ' || *p == 'R' || *p == 'W')
				p++;
			*lo = strtoull(p, &p, 16);
			*hi = strtoull(p, &p, 16);
			fclose(f);
			return 0;
		}
	fclose(f);
	return -1;
}

static void
pstack(int pid)
{
	char path[64];
	unsigned char regs[176];
	static unsigned char stk[STACK];
	u64 pc, sp, lo, hi, w;
	int fd, n, i;

	snprintf(path, sizeof path, "/proc/%d/regs", pid);
	fd = open(path, O_RDONLY);
	if(fd < 0 || read(fd, regs, sizeof regs) != sizeof regs){
		printf("pstack %d: no regs\n", pid);
		if(fd >= 0)
			close(fd);
		return;
	}
	close(fd);
	memcpy(&pc, regs + PCOFF, 8);
	memcpy(&sp, regs + SPOFF, 8);
	if(textseg(pid, &lo, &hi) < 0){
		printf("pstack %d: no text segment\n", pid);
		return;
	}
	printf("pstack %d pc %#llx sp %#llx\n", pid, pc, sp);
	snprintf(path, sizeof path, "/proc/%d/mem", pid);
	fd = open(path, O_RDONLY);
	if(fd < 0)
		return;
	n = pread(fd, stk, STACK, (off_t)sp);
	close(fd);
	for(i = 0; i + 8 <= n; i += 8){
		memcpy(&w, stk + i, 8);
		if(w >= lo && w < hi)
			printf("pstack %d ret %#llx\n", pid, w);
	}
}

int
main(int argc, char **argv)
{
	int i;

	for(i = 1; i < argc; i++)
		pstack(atoi(argv[i]));
	return 0;
}
