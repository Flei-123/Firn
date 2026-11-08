#include <cstdint>
#include <cstdio>

struct Op {
    uint64_t k;
    explicit Op(uint64_t key) : k(key) {}
    virtual uint64_t f(uint64_t x) const = 0;
    virtual ~Op() {}
};
struct Add : Op {
    using Op::Op;
    uint64_t f(uint64_t x) const override { return x + k; }
};
struct Xor : Op {
    using Op::Op;
    uint64_t f(uint64_t x) const override { return x ^ k; }
};
struct Mul : Op {
    using Op::Op;
    uint64_t f(uint64_t x) const override { return x * 3 + k; }
};

int main() {
    Op *ops[3] = {new Add(7), new Xor(11), new Mul(13)};
    for (int i = 0; i < 3; i++) asm volatile("" : "+r"(ops[i]));
    uint64_t s = 1;
    for (uint64_t i = 0; i < 50000000; i++) s = ops[i % 3]->f(s);
    printf("%llu\n", (unsigned long long)s);
    return 0;
}
