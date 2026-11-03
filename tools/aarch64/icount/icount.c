#include <glib.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <qemu-plugin.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;

typedef struct {
    uint64_t pc;
    uint64_t execs;
    uint32_t insns;
} TbStat;

static uint64_t total_insns;
static GPtrArray *tbs;
static int top_n;

static void on_tb_translate(qemu_plugin_id_t id, struct qemu_plugin_tb *tb)
{
    size_t n = qemu_plugin_tb_n_insns(tb);
    qemu_plugin_register_vcpu_tb_exec_inline(tb, QEMU_PLUGIN_INLINE_ADD_U64, &total_insns, n);
    if (top_n > 0) {
        TbStat *s = g_new0(TbStat, 1);
        s->pc = qemu_plugin_tb_vaddr(tb);
        s->insns = (uint32_t)n;
        g_ptr_array_add(tbs, s);
        qemu_plugin_register_vcpu_tb_exec_inline(tb, QEMU_PLUGIN_INLINE_ADD_U64, &s->execs, 1);
    }
}

static gint cmp_stat(gconstpointer a, gconstpointer b)
{
    const TbStat *x = *(TbStat *const *)a;
    const TbStat *y = *(TbStat *const *)b;
    uint64_t wx = x->execs * x->insns;
    uint64_t wy = y->execs * y->insns;
    return wx < wy ? 1 : (wx > wy ? -1 : 0);
}

static void on_exit_cb(qemu_plugin_id_t id, void *p)
{
    gchar *out = g_strdup_printf("INSNS %" PRIu64 "\n", total_insns);
    qemu_plugin_outs(out);
    g_free(out);
    if (top_n > 0) {
        g_ptr_array_sort(tbs, cmp_stat);
        for (guint i = 0; i < tbs->len && (int)i < top_n; i++) {
            TbStat *s = g_ptr_array_index(tbs, i);
            if (s->execs == 0) break;
            out = g_strdup_printf("TB %" PRIx64 " n=%u execs=%" PRIu64 " w=%" PRIu64 "\n", s->pc, s->insns, s->execs, s->execs * s->insns);
            qemu_plugin_outs(out);
            g_free(out);
        }
    }
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info, int argc, char **argv)
{
    tbs = g_ptr_array_new();
    for (int i = 0; i < argc; i++) {
        if (strncmp(argv[i], "top=", 4) == 0) top_n = atoi(argv[i] + 4);
    }
    qemu_plugin_register_vcpu_tb_trans_cb(id, on_tb_translate);
    qemu_plugin_register_atexit_cb(id, on_exit_cb, NULL);
    return 0;
}
