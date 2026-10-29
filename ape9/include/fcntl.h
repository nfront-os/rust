/* APE's <fcntl.h> with posix_fallocate (ape9/ext) */
#ifndef __APE9_FCNTL_H
#define __APE9_FCNTL_H
#include_next <fcntl.h>
#include <sys/types.h>
#ifdef __cplusplus
extern "C" {
#endif
extern int posix_fallocate(int, off_t, off_t);
#ifdef __cplusplus
}
#endif
#endif
