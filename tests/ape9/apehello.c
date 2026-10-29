/* C against APE in a Rust program (build/ape9) */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
#include <fcntl.h>

int
ape_hello(char *buf, int n)
{
	FILE *f;
	char *home;
	int fd, e;

	f = fopen("/dev/sysname", "r");
	if(f == NULL || fgets(buf, n, f) == NULL)
		return -1;
	fclose(f);
	home = getenv("home");
	fd = open("/nonexistent", O_RDONLY);
	e = errno;
	printf("ape9: sysname %s home %s open %d errno %d (%s)\n", buf, home ? home : "?", fd, e, strerror(e));
	fflush(stdout);
	return fd == -1 && e == ENOENT && home != NULL ? 0 : 1;
}
