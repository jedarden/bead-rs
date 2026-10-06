/* Independent build-environment witness for bundled SQLite's ARM sysroot. */
#include <assert.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <math.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

int main(int argc, char **argv) {
    pthread_mutex_t mutex = PTHREAD_MUTEX_INITIALIZER;
    void *library = dlopen(NULL, RTLD_NOW);
    double value = argc > 1 ? strtod(argv[1], NULL) : 2.0;
    assert(sizeof(uint64_t) == 8);
    if (pthread_mutex_lock(&mutex) != 0) {
        return EXIT_FAILURE;
    }
    int result = printf("cross-toolchain witness: %.2f\n", sqrt(value));
    if (library != NULL) {
        dlclose(library);
    }
    pthread_mutex_unlock(&mutex);
    pthread_mutex_destroy(&mutex);
    return result < 0 ? EXIT_FAILURE : EXIT_SUCCESS;
}
