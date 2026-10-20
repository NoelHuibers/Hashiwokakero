/* What MiniSat's DIMACS parser (minisat/utils/ParseUtils.h) needs from zlib: the Dojo hands it
   plain text on standard input, so a gzFile is a file descriptor and gzread is read. */
#ifndef DOJO_ZLIB_H
#define DOJO_ZLIB_H

#include <unistd.h>

typedef int gzFile;

static inline int gzread(gzFile file, void *buf, unsigned len) {
    return (int) read(file, buf, len);
}

#endif
