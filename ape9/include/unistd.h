/* APE's <unistd.h> with what libgit2 and sqlite want beyond it (ape9/ext) */
#ifndef __APE9_UNISTD_H
#define __APE9_UNISTD_H
#include_next <unistd.h>

#ifdef __cplusplus
extern "C" {
#endif
/* the file server has a write when it answers it */
extern int fsync(int);
extern int fdatasync(int);
/* Plan 9 has note groups, not sessions: the pid */
extern pid_t getsid(pid_t);
extern pid_t getpgid(pid_t);
/* Plan 9 gives files away only through wstat by their owner: no change, no error */
extern int fchown(int, uid_t, gid_t);
#ifdef __cplusplus
}
#endif

/* sysconf: unknown to APE's, -1 */
#ifndef _SC_GETPW_R_SIZE_MAX
#define _SC_GETPW_R_SIZE_MAX	1000
#endif
#ifndef _SC_GETGR_R_SIZE_MAX
#define _SC_GETGR_R_SIZE_MAX	1001
#endif
#endif
