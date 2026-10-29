/*
 * What C libraries (libgit2, sqlite, curl) want of POSIX beyond APE: over APE's own.
 */
#include <sys/types.h>
#include <sys/time.h>
#include <unistd.h>
#include <utime.h>
#include <pwd.h>
#include <string.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>

/* a write is done when the file server answers it */
int
fsync(int fd)
{
	(void)fd;
	return 0;
}

int
fdatasync(int fd)
{
	(void)fd;
	return 0;
}

/* note groups, not sessions */
pid_t
getsid(pid_t pid)
{
	return pid == 0 ? getpid() : pid;
}

pid_t
getpgid(pid_t pid)
{
	return pid == 0 ? getpid() : pid;
}

/* seconds: stat(5) has no more */
int
utimes(const char *path, const struct timeval *tv)
{
	struct utimbuf u, *up;

	up = NULL;
	if(tv != NULL){
		u.actime = tv[0].tv_sec;
		u.modtime = tv[1].tv_sec;
		up = &u;
	}
	return utime(path, up);
}

static int
pwcopy(struct passwd *p, struct passwd *pw, char *buf, size_t n, struct passwd **res)
{
	char *f[4], **d[4];
	size_t l;
	int i;

	*res = NULL;
	if(p == NULL)
		return errno ? errno : ENOENT;
	*pw = *p;
	f[0] = p->pw_name; d[0] = &pw->pw_name;
	f[1] = p->pw_dir; d[1] = &pw->pw_dir;
	f[2] = p->pw_shell; d[2] = &pw->pw_shell;
	f[3] = NULL;
	for(i = 0; f[i] != NULL; i++){
		l = strlen(f[i]) + 1;
		if(l > n)
			return ERANGE;
		memcpy(buf, f[i], l);
		*d[i] = buf;
		buf += l;
		n -= l;
	}
	*res = pw;
	return 0;
}

int
getpwuid_r(uid_t uid, struct passwd *pw, char *buf, size_t n, struct passwd **res)
{
	return pwcopy(getpwuid(uid), pw, buf, n, res);
}

int
getpwnam_r(const char *name, struct passwd *pw, char *buf, size_t n, struct passwd **res)
{
	return pwcopy(getpwnam(name), pw, buf, n, res);
}

int
fchown(int fd, uid_t uid, gid_t gid)
{
	(void)fd; (void)uid; (void)gid;
	return 0;
}

/* the file at least off+len long: no blocks to reserve on Plan 9's file servers */
int
posix_fallocate(int fd, off_t off, off_t len)
{
	struct stat st;

	if(fstat(fd, &st) < 0)
		return errno;
	if(st.st_size < off + len && ftruncate(fd, off + len) < 0)
		return errno;
	return 0;
}
