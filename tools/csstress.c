/* csstress nproc ndial: four listeners on tcp!127.0.0.1!port accepting and closing, nproc
 * procs (rfork RFMEM, as Rust's threads) each dialing it ndial times through dial(2),
 * that is /net/cs; every 2 s the count of dials done. Prints "csstress: done" at the end. */
#include <u.h>
#include <libc.h>

static long done;
static char addr[64];

static void
listener(char *adir)
{
	char ldir[40];
	int lcfd, dfd;

	for(;;){
		lcfd = listen(adir, ldir);
		if(lcfd < 0)
			sysfatal("listen: %r");
		dfd = accept(lcfd, ldir);
		close(lcfd);
		if(dfd >= 0)
			close(dfd);
	}
}

void
main(int argc, char **argv)
{
	int nproc, ndial, i, j, afd, fd, port;
	char adir[40], buf[64];
	long last;
	int idle;

	nproc = argc > 1 ? atoi(argv[1]) : 12;
	ndial = argc > 2 ? atoi(argv[2]) : 200;
	afd = announce("tcp!127.0.0.1!0", adir);
	if(afd < 0)
		sysfatal("announce: %r");
	snprint(buf, sizeof buf, "%s/local", adir);
	fd = open(buf, OREAD);
	i = read(fd, buf, sizeof buf - 1);
	close(fd);
	buf[i > 0 ? i : 0] = 0;
	port = atoi(strchr(buf, '!') + 1);
	snprint(addr, sizeof addr, "tcp!127.0.0.1!%d", port);
	print("csstress: %s, %d procs x %d dials\n", addr, nproc, ndial);
	/* several listeners: one alone lets the kernel's queue of calls fill (Maxincall 10) */
	for(i = 0; i < 4; i++)
		if(rfork(RFPROC|RFMEM|RFNOWAIT) == 0){
			listener(adir);
			exits(nil);
		}
	for(i = 0; i < nproc; i++)
		if(rfork(RFPROC|RFMEM|RFNOWAIT) == 0){
			for(j = 0; j < ndial; j++){
				fd = dial(addr, nil, nil, nil);
				if(fd < 0)
					fprint(2, "dial: %r\n");
				else
					close(fd);
				ainc(&done);
			}
			exits(nil);
		}
	last = -1;
	idle = 0;
	while(done < (long)nproc*ndial){
		sleep(2000);
		if(done == last){
			idle += 2;
			if(idle == 2 || idle % 10 == 0)
				print("csstress: %ld, no progress for %d s\n", done, idle);
			if(idle >= 60){
				print("csstress: STUCK at %ld\n", done);
				exits("stuck");
			}
		}else
			idle = 0;
		last = done;
	}
	print("csstress: done\n");
	exits(nil);
}
