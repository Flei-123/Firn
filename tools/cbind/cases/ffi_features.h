#ifndef FFI_FEATURES_H
#define FFI_FEATURES_H
typedef struct hdr { unsigned char tag; unsigned int len; unsigned short flags; } hdr;
enum { HDR_SIZE = sizeof(hdr), PTR_SIZE = sizeof(void *), ULONG_SIZE = sizeof(unsigned long), SHORT_SIZE = sizeof(short int) };
#define HDR_BYTES sizeof(struct hdr)
#define BUF_BYTES (sizeof(hdr) * 4 + 1)
#define NAME_STR "fire\tfly"
#define VERSION_STR "1." "2"
#define PI_F 3.14159
#define NEG_E -2.5e-3
#define HALF .5f
struct bits {
    unsigned a : 3;
    int b : 5;
    unsigned c : 1;
    unsigned d : 17;
    unsigned char e;
    int f : 6;
    unsigned g : 9;
};
int vsum(int n, ...);
double dmix(const char *tag, int n, ...);
#endif
