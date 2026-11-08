#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <vector>

int main() {
    std::vector<uint32_t> v;
    uint64_t state = 88172645463325252ULL;
    for (int i = 0; i < 1000000; i++) {
        state = state * 6364136223846793005ULL + 1442695040888963407ULL;
        v.push_back((uint32_t)(state >> 33));
    }
    std::sort(v.begin(), v.end());
    uint64_t check = 0;
    for (size_t i = 0; i < v.size(); i += 1024) check = check * 31 + v[i];
    printf("%llu\n", (unsigned long long)check);
    return 0;
}
