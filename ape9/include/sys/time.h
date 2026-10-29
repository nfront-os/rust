/* APE's <sys/time.h> with utimes (ape9/ext) */
#ifndef __APE9_SYS_TIME_H
#define __APE9_SYS_TIME_H
#include_next <sys/time.h>
#ifdef __cplusplus
extern "C" {
#endif
extern int utimes(const char *, const struct timeval *);
#ifdef __cplusplus
}
#endif
#endif
