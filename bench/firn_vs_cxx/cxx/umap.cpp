#include <cstdint>
#include <cstdio>
#include <unordered_map>

int main() {
    std::unordered_map<uint64_t, uint64_t> m;
    uint64_t state = 88172645463325252ULL;
    for (uint64_t i = 0; i < 500000; i++) {
        state = state * 6364136223846793005ULL + 1442695040888963407ULL;
        m[state >> 20] = i;
    }
    uint64_t hits = 0, sum = 0;
    state = 88172645463325252ULL;
    for (uint64_t i = 0; i < 1000000; i++) {
        state = state * 6364136223846793005ULL + 1442695040888963407ULL;
        auto it = m.find(state >> 20);
        if (it != m.end()) {
            hits++;
            sum += it->second;
        }
    }
    printf("%llu %llu\n", (unsigned long long)hits, (unsigned long long)sum);
    return 0;
}
