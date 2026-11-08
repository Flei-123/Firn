#include <cstdint>
#include <cstdio>
#include <vector>

int main() {
    std::vector<uint64_t> v;
    for (uint64_t i = 0; i < 2000000; i++) v.push_back(i * 2654435761ULL);
    uint64_t sum = 0;
    for (uint64_t r = 0; r < 20; r++)
        for (size_t j = 0; j < v.size(); j++) sum += v[j] ^ r;
    printf("%llu\n", (unsigned long long)sum);
    return 0;
}
