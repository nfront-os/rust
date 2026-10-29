/* exit and abort from C in a Rust program: the whole program ends (std's __plan9_exit) */
#include <stdio.h>
#include <stdlib.h>

void
c_exit(int how)
{
	printf("cexit: from C, %s\n", how ? "abort" : "exit(3)");
	fflush(stdout);
	if(how)
		abort();
	exit(3);
}
