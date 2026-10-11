// aulos_offline.cpp -- runs a scenario script against the real Aulos library, offline.
// Build: see tools/audio/aulos/run.sh. Usage: aulos_offline SCRIPT ASSET_DIR OUT.f32
// Output: raw interleaved stereo float32 of every `render`, and a stats line on stdout.
#include "aulos.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

struct Ev {
    std::string name, bus;
    std::vector<std::string> files;
    std::map<std::string, std::string> kv;
    struct P { std::string name, target, def; std::vector<std::pair<std::string, std::string>> pts; };
    std::vector<P> params;
};

static std::vector<std::string> split(const std::string &s, char c) {
    std::vector<std::string> o; std::stringstream ss(s); std::string t;
    while (std::getline(ss, t, c)) o.push_back(t);
    return o;
}

int main(int argc, char **argv) {
    if (argc < 4) { std::fprintf(stderr, "usage: aulos_offline SCRIPT ASSETS OUT\n"); return 2; }
    std::ifstream in(argv[1]);
    std::vector<std::vector<std::string>> lines;
    std::string ln;
    uint32_t rate = 48000, voices = 64;
    std::vector<std::pair<std::string, std::string>> buses;
    std::vector<Ev> events;
    while (std::getline(in, ln)) {
        size_t h = ln.find('#'); if (h != std::string::npos) ln = ln.substr(0, h);
        std::stringstream ss(ln); std::vector<std::string> t; std::string w;
        while (ss >> w) t.push_back(w);
        if (t.empty()) continue;
        if (t[0] == "rate") rate = (uint32_t)std::atoi(t[1].c_str());
        else if (t[0] == "voices") voices = (uint32_t)std::atoi(t[1].c_str());
        else if (t[0] == "bus") buses.push_back({t[1], t.size() > 2 ? t[2] : ""});
        else if (t[0] == "event") {
            Ev e; e.name = t[1]; e.bus = t[2]; e.files = split(t[3], ',');
            for (size_t i = 4; i < t.size(); ++i) { size_t q = t[i].find('='); e.kv[t[i].substr(0, q)] = t[i].substr(q + 1); }
            events.push_back(e);
        } else if (t[0] == "param") {
            for (auto &e : events) if (e.name == t[1]) {
                Ev::P p; p.name = t[2]; p.target = t[3]; p.def = t[4];
                for (auto &pt : split(t[5], ',')) { auto xy = split(pt, ':'); p.pts.push_back({xy[0], xy[1]}); }
                e.params.push_back(p);
            }
        } else lines.push_back(t);
    }
    // bank JSON (strict, so the Firn loader reads the very same text)
    std::string j = "{\"buses\":[{\"name\":\"master\",\"volume\":1.0}";
    for (auto &b : buses) {
        if (b.first == "master") continue;
        j += ",{\"name\":\"" + b.first + "\"";
        if (!b.second.empty()) j += ",\"parent\":\"" + b.second + "\"";
        j += ",\"volume\":1.0}";
    }
    j += "],\"events\":[";
    for (size_t i = 0; i < events.size(); ++i) {
        auto &e = events[i];
        if (i) j += ",";
        j += "{\"name\":\"" + e.name + "\",\"bus\":\"" + e.bus + "\",\"samples\":[";
        for (size_t k = 0; k < e.files.size(); ++k) j += (k ? ",\"" : "\"") + e.files[k] + "\"";
        j += "]";
        static const char *num[][2] = {{"volume","volume"},{"pitch","pitch"},{"vol_rand","volume_random"},
            {"pitch_rand","pitch_random"},{"priority","priority"},{"min","min_distance"},{"max","max_distance"},
            {"factor","rolloff_factor"},{"doppler","doppler"},{"maxinst","max_instances"},{"lowpass","lowpass"}};
        for (auto &n : num) if (e.kv.count(n[0])) j += std::string(",\"") + n[1] + "\":" + e.kv[n[0]];
        if (e.kv.count("loop")) j += std::string(",\"loop\":") + (e.kv["loop"] == "1" ? "true" : "false");
        if (e.kv.count("spatial")) j += std::string(",\"spatial\":") + (e.kv["spatial"] == "1" ? "true" : "false");
        if (e.kv.count("rolloff")) j += std::string(",\"rolloff\":\"") + (e.kv["rolloff"] == "1" ? "linear" : "inverse") + "\"";
        if (!e.params.empty()) {
            j += ",\"parameters\":[";
            for (size_t k = 0; k < e.params.size(); ++k) {
                auto &p = e.params[k];
                j += std::string(k ? "," : "") + "{\"name\":\"" + p.name + "\",\"target\":\"" + p.target + "\",\"default\":" + p.def + ",\"curve\":[";
                for (size_t m = 0; m < p.pts.size(); ++m) j += std::string(m ? "," : "") + "[" + p.pts[m].first + "," + p.pts[m].second + "]";
                j += "]}";
            }
            j += "]";
        }
        j += "}";
    }
    j += "]}";
    std::string bankPath = std::string(argv[3]) + ".bank.json";
    { std::ofstream b(bankPath); b << j; }

    aul_config cfg = {}; cfg.sample_rate = rate; cfg.max_voices = voices; cfg.enable_device = 0; cfg.asset_root = argv[2];
    aul_system *sys = nullptr;
    if (aul_create(&cfg, &sys) != AUL_OK) { std::fprintf(stderr, "create failed\n"); return 1; }
    if (aul_load_bank(sys, bankPath.c_str()) != AUL_OK) { std::fprintf(stderr, "bank: %s\n", aul_last_error(sys)); return 1; }
    for (auto &b : buses) aul_ensure_bus(sys, b.first.c_str(), b.second.empty() ? nullptr : b.second.c_str());

    FILE *out = std::fopen(argv[3], "wb");
    std::vector<aul_instance> hs;
    auto F = [](const std::string &s) { return (float)std::atof(s.c_str()); };
    for (auto &t : lines) {
        const std::string &c = t[0];
        if (c == "listener") {
            aul_set_listener(sys, {F(t[1]),F(t[2]),F(t[3])}, {F(t[4]),F(t[5]),F(t[6])}, {F(t[7]),F(t[8]),F(t[9])}, {F(t[10]),F(t[11]),F(t[12])});
        } else if (c == "play") {
            hs.push_back(t.size() > 2 ? aul_play_on_bus(sys, t[1].c_str(), t[2].c_str()) : aul_play(sys, t[1].c_str()));
        } else if (c == "play3d") {
            aul_vec3 p = {F(t[2]), F(t[3]), F(t[4])};
            hs.push_back(t.size() > 5 ? aul_play_3d_on_bus(sys, t[1].c_str(), p, t[5].c_str()) : aul_play_3d(sys, t[1].c_str(), p));
        } else if (c == "stop") aul_stop(sys, hs[std::atoi(t[1].c_str())], F(t[2]));
        else if (c == "stopall") aul_stop_all(sys, F(t[1]));
        else if (c == "setvol") aul_set_volume(sys, hs[std::atoi(t[1].c_str())], F(t[2]));
        else if (c == "setpitch") aul_set_pitch(sys, hs[std::atoi(t[1].c_str())], F(t[2]));
        else if (c == "setpos") aul_set_position(sys, hs[std::atoi(t[1].c_str())], {F(t[2]),F(t[3]),F(t[4])}, {F(t[5]),F(t[6]),F(t[7])});
        else if (c == "setparam") aul_set_parameter(sys, hs[std::atoi(t[1].c_str())], t[2].c_str(), F(t[3]));
        else if (c == "mute") aul_set_bus_mute(sys, t[1].c_str(), std::atoi(t[2].c_str()));
        else if (c == "busvol") aul_set_bus_volume(sys, t[1].c_str(), F(t[2]));
        else if (c == "update") aul_update(sys);
        else if (c == "render") {
            uint32_t n = (uint32_t)std::atoi(t[1].c_str());
            std::vector<float> buf((size_t)n * 2);
            aul_render(sys, buf.data(), n);
            std::fwrite(buf.data(), 4, buf.size(), out);
        } else { std::fprintf(stderr, "unknown command %s\n", c.c_str()); return 1; }
    }
    std::fclose(out);
    aul_stats s; aul_get_stats(sys, &s);
    std::printf("STATS active=%u started=%llu stolen=%llu dropped=%llu\n", s.active_voices,
                (unsigned long long)s.started, (unsigned long long)s.stolen, (unsigned long long)s.dropped);
    aul_destroy(sys);
    return 0;
}
