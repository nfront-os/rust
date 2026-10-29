/* APE's <netinet/in.h> with the IPv6 address tests of RFC 3493 it lacks */
#ifndef __APE9_NETINET_IN_H
#define __APE9_NETINET_IN_H
#include_next <netinet/in.h>
#ifndef IN6_IS_ADDR_LINKLOCAL
#define IN6_IS_ADDR_LINKLOCAL(a) \
	(((const unsigned char *)(a))[0] == 0xfe && (((const unsigned char *)(a))[1] & 0xc0) == 0x80)
#endif
#ifndef IN6_IS_ADDR_SITELOCAL
#define IN6_IS_ADDR_SITELOCAL(a) \
	(((const unsigned char *)(a))[0] == 0xfe && (((const unsigned char *)(a))[1] & 0xc0) == 0xc0)
#endif
#ifndef IN6_IS_ADDR_MULTICAST
#define IN6_IS_ADDR_MULTICAST(a) (((const unsigned char *)(a))[0] == 0xff)
#endif
#endif
