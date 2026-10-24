ctx_module_of() {
    local m=$1
    case "$m" in
        lib/@*/*) m=${m#lib/@*/} ;;
        *) m=${m#lib/} ;;
    esac
    m=${m%.fi}
    m=${m%.windows}
    echo "${m//\//.}"
}

ctx_flags() {
    case "$1" in
        lib/@android/*|lib/android/*|lib/plat/android/*) echo "--target=aarch64-android --pic -c" ;;
        lib/@windows/*|*.windows.fi) echo "--target=x86_64-windows" ;;
        lib/@web/*) echo "--target=wasm32-browser" ;;
        *) echo "" ;;
    esac
}

ctx_is_special() {
    case "$1" in
        lib/rc/parts/*) return 0 ;;
    esac
    head -30 "$1" | grep -q '^// Include:'
}

ctx_prelude() {
    case "$1" in
        bare) ;;
        prelude) printf 'import std.rt\nimport std.str\n' ;;
        window) printf 'import std.rt\nimport std.str\nimport window.window\nimport window.backend\n' ;;
    esac
}

ctx_build() {
    local f=$1 mode=$2 mod
    mod=$(ctx_module_of "$f")
    [ "$mode" = window ] && [ "$mod" = window.x11 ] && mod=window.backend
    { ctx_prelude "$mode"; [ "$mode" = window ] && [ "$mod" = window.backend ] || printf 'import %s\n' "$mod"; printf 'fn main() -> i32 {\n    return 0\n}\n'; } > "$CTX_T/m.fi"
    timeout 180 "$FIRNC" $(ctx_flags "$f") -o "$CTX_T/m.out" "$CTX_T/m.fi" > /dev/null 2>&1
}

ctx_compiles() {
    ctx_build "$1" "$CTX_MODE"
}

ctx_probe() {
    local f=$1 mode
    ctx_is_special "$f" && return 1
    local modes="bare prelude"
    case "$f" in lib/window/*) modes="bare prelude window" ;; esac
    for mode in $modes; do
        if ctx_build "$f" "$mode"; then
            CTX_MODE=$mode
            return 0
        fi
    done
    return 1
}
