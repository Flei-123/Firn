# SPDX-License-Identifier: MPL-2.0
NEW_NAME_PATTERN='^[a-z][a-z0-9_]{0,39}$'

new_parse_args() {
    NEW_NAME=""
    NEW_DIR=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --dir) NEW_DIR=${2:?--dir needs a value}; shift 2 ;;
            -*) fui_die 2 "unknown option '$1'" ;;
            *) [ -z "$NEW_NAME" ] || fui_die 2 "one name only"; NEW_NAME=$1; shift ;;
        esac
    done
    [ -n "$NEW_NAME" ] || fui_die 2 "usage: fui new <name> [--dir DIR]"
    [[ "$NEW_NAME" =~ $NEW_NAME_PATTERN ]] || fui_die 2 "name must be lower case letters, digits and '_', starting with a letter"
    NEW_DIR=${NEW_DIR:-$NEW_NAME}
}

new_write_file() {
    local template=$1 target=$2
    mkdir -p "$(dirname "$target")"
    sed -e "s/@NAME@/$NEW_NAME/g" "$template" > "$target"
}

verb_new() {
    new_parse_args "$@"
    if [ -e "$NEW_DIR" ] && [ -n "$(ls -A "$NEW_DIR" 2>/dev/null)" ]; then
        fui_die 2 "$NEW_DIR exists and is not empty"
    fi
    local t=$FUI_CLI/template
    new_write_file "$t/firn.package.in" "$NEW_DIR/firn.package"
    new_write_file "$t/fui.app.in" "$NEW_DIR/fui.app"
    new_write_file "$t/main.fi.in" "$NEW_DIR/main.fi"
    new_write_file "$t/gitignore.in" "$NEW_DIR/.gitignore"
    new_write_file "$t/data/greeting.json.in" "$NEW_DIR/data/greeting.json"
    fui_info "created $NEW_DIR (next: cd $NEW_DIR && fui build --targets native,web --verify)"
}
