#!/usr/bin/env bash
BRAND_ROOT=${BRAND_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}

brand_read() {
    local section="" line key value
    while IFS= read -r line || [ -n "$line" ]; do
        line=${line%%#*}
        line=$(printf '%s' "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
        [ -z "$line" ] && continue
        case "$line" in
            "["*"]") section=${line#[}; section=${section%]}; continue ;;
        esac
        key=${line%%=*}
        value=${line#*=}
        key=$(printf '%s' "$key" | sed 's/[[:space:]]*$//')
        value=$(printf '%s' "$value" | sed 's/^[[:space:]]*"//;s/"[[:space:]]*$//')
        if [ -z "$section" ]; then
            printf 'BRAND_%s=%q\n' "$(printf '%s' "$key" | tr a-z A-Z)" "$value"
        elif [ "$section" = legacy ]; then
            printf 'BRAND_LEGACY_%s=%q\n' "$(printf '%s' "$key" | tr a-z A-Z)" "$value"
        fi
    done < "$BRAND_ROOT/brand.toml"
}

eval "$(brand_read)"
BRAND_SYSROOT_ENV=${BRAND_ENV_PREFIX}LIB
BRAND_HOME_ENV=${BRAND_ENV_PREFIX}_HOME
BRAND_HOME_DIR=.$BRAND_CLI
export BRAND_LANG_NAME BRAND_CLI BRAND_COMPILER BRAND_EXT BRAND_ENV_PREFIX BRAND_SYMBOL_PREFIX \
    BRAND_PACKAGE_FILE BRAND_LOCK_FILE BRAND_TOOLCHAIN_FILE BRAND_SYSROOT_ENV BRAND_HOME_ENV BRAND_HOME_DIR \
    BRAND_LEGACY_ENV_PREFIX BRAND_LEGACY_PACKAGE_FILE BRAND_LEGACY_LOCK_FILE BRAND_LEGACY_CLI

brand_env() { printf '%s_%s' "$BRAND_ENV_PREFIX" "$1"; }

brand_emit_fi() {
    cat <<EOT
const LANG_NAME: str = "$BRAND_LANG_NAME"
const CLI: str = "$BRAND_CLI"
const COMPILER: str = "$BRAND_COMPILER"
const ENV_PREFIX: str = "$BRAND_ENV_PREFIX"
const TOOLCHAIN_FILE: str = "$BRAND_TOOLCHAIN_FILE"
const LEGACY_ENV_PREFIX: str = "$BRAND_LEGACY_ENV_PREFIX"
const LEGACY_CLI: str = "$BRAND_LEGACY_CLI"
EOT
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    case "${1:-}" in
        --emit-fi) brand_emit_fi ;;
        --check-fi)
            brand_emit_fi | cmp -s - "$BRAND_ROOT/lib/toolchain/tcbrand.fi" || { echo "lib/toolchain/tcbrand.fi is stale: run tools/brand.sh --emit-fi > lib/toolchain/tcbrand.fi" >&2; exit 1; }
            ;;
        --get) eval "printf '%s\n' \"\${BRAND_$(printf '%s' "${2:?key}" | tr a-z A-Z)}\"" ;;
        *) env | grep '^BRAND_' | sort ;;
    esac
fi
