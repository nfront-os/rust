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

#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <poll.h>

/* poll on an APE socket: nothing for 200 ms, then data, then the end */
int
ape_poll(int port)
{
	struct sockaddr_in a;
	struct pollfd p;
	char buf[64];
	int s, r, n;

	s = socket(AF_INET, SOCK_STREAM, 0);
	if(s < 0)
		return -1;
	memset(&a, 0, sizeof a);
	a.sin_family = AF_INET;
	a.sin_port = htons(port);
	a.sin_addr.s_addr = inet_addr("127.0.0.1");
	if(connect(s, (struct sockaddr*)&a, sizeof a) < 0)
		return -2;
	p.fd = s;
	p.events = POLLIN;
	r = poll(&p, 1, 200);
	if(r != 0)
		return -3;
	if(write(s, "go\n", 3) != 3)
		return -4;
	r = poll(&p, 1, 5000);
	if(r != 1 || !(p.revents & POLLIN))
		return -5;
	n = read(s, buf, sizeof buf - 1);
	if(n <= 0)
		return -6;
	buf[n] = 0;
	r = poll(&p, 1, 5000);
	if(r != 1){
		printf("ape9: eof poll %d revents %x\n", r, p.revents);
		return -7;
	}
	if((r = read(s, buf + n, 1)) != 0){
		printf("ape9: eof read %d errno %d\n", r, errno);
		return -9;
	}
	close(s);
	printf("ape9: poll got %s", buf);
	fflush(stdout);
	return strcmp(buf, "data\n") == 0 ? 0 : -8;
}
