/* APE's <pwd.h> with the reentrant lookups (ape9/ext) */
#ifndef __APE9_PWD_H
#define __APE9_PWD_H
#include_next <pwd.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
extern int getpwuid_r(uid_t, struct passwd *, char *, size_t, struct passwd **);
extern int getpwnam_r(const char *, struct passwd *, char *, size_t, struct passwd **);
#ifdef __cplusplus
}
#endif
#endif
