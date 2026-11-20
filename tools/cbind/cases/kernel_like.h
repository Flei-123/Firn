#ifndef KERNEL_LIKE_H
#define KERNEL_LIKE_H
#define KL_VERSION 0x0305
#define KL_MAX_NAME 32
#define KL_FLAG_A (1 << 3)
#define KL_FLAG_B (KL_FLAG_A | 0x10)
#ifdef KL_HAVE_PCI
#define KL_PCI 1
#else
#define KL_PCI 0
#endif

typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;
typedef unsigned long long u64;

struct list_head {
    struct list_head *next, *prev;
};

struct hlist_node {
    struct hlist_node *next, **pprev;
};

struct device_ops {
    int (*probe)(void *dev);
    void (*remove)(void *dev, int flags);
    const char *name;
};

struct pci_dev {
    struct list_head bus_list;
    u16 vendor;
    u16 device;
    u8 revision;
    u8 hdr_type;
    u32 class_code;
    unsigned int irq;
    char name[KL_MAX_NAME];
    u64 resource[3][2];
    struct device_ops *ops;
    unsigned int enabled:1;
    unsigned int is_busmaster:1;
    unsigned int msi_cap:6;
    union {
        u32 word;
        u8 bytes[4];
    } cfg;
    struct {
        u16 lo, hi;
    } span;
    double scale;
    float ratio;
};

struct __attribute__((packed)) wire_hdr {
    u8 kind;
    u16 len;
    u32 seq;
    u8 data[];
};

struct aligned_blk {
    char tag;
    long long v __attribute__((aligned(16)));
};

union value {
    long l;
    double d;
    char buf[12];
};

typedef enum { KL_OK = 0, KL_ERR = -1, KL_BUSY = 16 } kl_status;

typedef struct kl_node {
    int key;
    struct kl_node *left, *right;
    kl_status st;
} kl_node;

int kl_insert(kl_node **root, int key);
kl_node *kl_find(const kl_node *root, int key);
void kl_free(kl_node *root);
int kl_walk(const kl_node *root, int (*visit)(const kl_node *, void *), void *ctx);
unsigned long kl_hash(const void *data, unsigned long len, unsigned int seed);
double kl_scale(double v, float f, int n);
struct list_head kl_head(void);
int kl_logf(const char *fmt, ...);
#endif
