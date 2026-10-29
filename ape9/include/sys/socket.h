/* APE's <sys/socket.h> with its socket options (HAVE_SOCK_OPTS) and fd_set, as POSIX has */
#ifndef __APE9_SYS_SOCKET_H
#define __APE9_SYS_SOCKET_H
#ifndef HAVE_SOCK_OPTS
#define HAVE_SOCK_OPTS
#endif
#include_next <sys/socket.h>
#include <sys/select.h>
#endif
