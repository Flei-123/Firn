# SPDX-License-Identifier: MPL-2.0
MANIFEST_OPT_LEVELS="dev dev-fast release-safe release-fast"

manifest_defaults() {
    APP_TITLE=$APP_PACKAGE
    APP_SIZE=1100x700
    APP_DATA_REL=data
    APP_TARGETS="native web"
    APP_ANDROID_ID=com.fleitec.fui.$APP_PACKAGE
    APP_OPT=release-safe
    APP_LIMIT=0.5
}

manifest_read_package() {
    local dir=$1 info
    [ -f "$dir/firn.package" ] || fui_die 5 "no firn.package in $dir (create a project with: fui new <name>)"
    info=$("$FIRNC" --package-info "$dir" 2>&1) || fui_die 5 "invalid firn.package in $dir: $info"
    APP_PACKAGE=$(awk '$1=="package"{print $2}' <<<"$info")
    APP_VERSION=$(awk '$1=="version"{print $2}' <<<"$info")
    APP_START=$(awk '$1=="start"{print $2}' <<<"$info")
    [ -n "$APP_START" ] || fui_die 5 "firn.package in $dir has no 'start' line (a library cannot be built as an app)"
}

manifest_set() {
    local key=$1 value=$2
    case "$key" in
        title) APP_TITLE=$value ;;
        size) APP_SIZE=$value ;;
        data) APP_DATA_REL=$value ;;
        targets) APP_TARGETS=$value ;;
        android_id) APP_ANDROID_ID=$value ;;
        opt) APP_OPT=$value ;;
        limit) APP_LIMIT=$value ;;
        *) fui_die 5 "$APP_DIR/fui.app: unknown key '$key'" ;;
    esac
}

manifest_read_app() {
    local file=$1 key value
    [ -f "$file" ] || return 0
    while read -r key value; do
        case "$key" in
            ""|\#*) continue ;;
        esac
        manifest_set "$key" "$value"
    done < "$file"
}

manifest_validate() {
    local t
    [[ "$APP_SIZE" =~ ^[0-9]+x[0-9]+$ ]] || fui_die 5 "fui.app: size must look like 1100x700, got '$APP_SIZE'"
    [[ " $MANIFEST_OPT_LEVELS " == *" $APP_OPT "* ]] || fui_die 5 "fui.app: opt must be one of: $MANIFEST_OPT_LEVELS"
    [[ "$APP_LIMIT" =~ ^[0-9]+(\.[0-9]+)?$ ]] || fui_die 5 "fui.app: limit must be a percentage like 0.5, got '$APP_LIMIT'"
    for t in $APP_TARGETS; do
        fui_is_target "$t" || fui_die 5 "fui.app: unknown target '$t' (known: $(fui_registered_targets | tr '\n' ' '))"
    done
}

manifest_load() {
    APP_DIR=$(cd "$1" 2>/dev/null && pwd) || fui_die 5 "no such directory: $1"
    manifest_read_package "$APP_DIR"
    manifest_defaults
    manifest_read_app "$APP_DIR/fui.app"
    manifest_validate
    APP_DATA_DIR=$(realpath -m "$APP_DIR/$APP_DATA_REL")
    case "$APP_START" in
        /*) ;;
        *) APP_START=$APP_DIR/$APP_START ;;
    esac
    APP_W=${APP_SIZE%x*}
    APP_H=${APP_SIZE#*x}
}
