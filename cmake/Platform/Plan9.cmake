# CMake's platform 9front (Plan 9), for C and C++ against APE, its POSIX layer (9front-gcc's
# x86_64-pc-plan9-gcc), in Rust programs: unix-like, static only.
include(Platform/UnixPaths)
set(UNIX 1)
set(CMAKE_EXECUTABLE_SUFFIX "")
set(CMAKE_STATIC_LIBRARY_PREFIX "lib")
set(CMAKE_STATIC_LIBRARY_SUFFIX ".a")
set(CMAKE_SHARED_LIBRARY_PREFIX "lib")
set(CMAKE_SHARED_LIBRARY_SUFFIX ".so")
set_property(GLOBAL PROPERTY TARGET_SUPPORTS_SHARED_LIBS FALSE)
set(CMAKE_FIND_LIBRARY_SUFFIXES ".a")
