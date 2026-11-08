#include <cstdint>
#include <cstdio>
#include <string>

int main() {
    std::string s;
    for (uint64_t i = 0; i < 2000000; i++) s.push_back((char)('a' + i % 26));
    for (uint64_t i = 0; i < 400000; i++) s.append("abcdefgh", 8);
    uint64_t check = 0;
    for (size_t i = 0; i < s.size(); i++) check = check * 131 + (unsigned char)s[i];
    printf("%zu\n%llu\n", s.size(), (unsigned long long)check);
    return 0;
}
