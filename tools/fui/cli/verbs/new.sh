# SPDX-License-Identifier: MPL-2.0
NEW_NAME_PATTERN='^[a-z][a-z0-9_]{0,39}$'
NEW_DEFAULT_TEMPLATE=counter

new_templates() {
    local d
    for d in "$FUI_CLI"/template/*/; do
        d=$(basename "$d")
        [ "$d" = common ] || echo "$d"
    done
}

new_parse_args() {
    NEW_NAME=""
    NEW_DIR=""
    NEW_TEMPLATE=$NEW_DEFAULT_TEMPLATE
    while [ $# -gt 0 ]; do
        case "$1" in
            --dir) NEW_DIR=${2:?--dir needs a value}; shift 2 ;;
            --template) NEW_TEMPLATE=${2:?--template needs a value}; shift 2 ;;
            -*) fui_die 2 "unknown option '$1'" ;;
            *) [ -z "$NEW_NAME" ] || fui_die 2 "one name only"; NEW_NAME=$1; shift ;;
        esac
    done
    [ -n "$NEW_NAME" ] || fui_die 2 "usage: fui new <name> [--template $(new_templates | paste -sd'|')] [--dir DIR]"
    [[ "$NEW_NAME" =~ $NEW_NAME_PATTERN ]] || fui_die 2 "name must be lower case letters, digits and '_', starting with a letter"
    [ "$NEW_TEMPLATE" != common ] && [ -d "$FUI_CLI/template/$NEW_TEMPLATE" ] \
        || fui_die 2 "unknown template '$NEW_TEMPLATE' (available: $(new_templates | paste -sd' '))"
    NEW_DIR=${NEW_DIR:-$NEW_NAME}
}

new_write_file() {
    local template=$1 target=$2
    mkdir -p "$(dirname "$target")"
    sed -e "s/@NAME@/$NEW_NAME/g" -e "s/@SIZE@/$NEW_SIZE/g" "$template" > "$target"
}

new_write_tree() {
    local root=$1 file rel target
    while IFS= read -r file; do
        rel=${file#"$root"/}
        case "$rel" in size|source) continue ;; esac
        target=$NEW_DIR/${rel%.in}
        case "$rel" in gitignore.in) target=$NEW_DIR/.gitignore ;; esac
        new_write_file "$file" "$target"
    done < <(find "$root" -type f | sort)
}

new_copy_source() {
    local src=$FUI_ROOT/$(tr -d '\n' < "$1/source")
    [ -d "$src" ] || fui_die 1 "template source $src is missing"
    cp "$src"/*.fi "$NEW_DIR/"
}

verb_new() {
    new_parse_args "$@"
    if [ -e "$NEW_DIR" ] && [ -n "$(ls -A "$NEW_DIR" 2>/dev/null)" ]; then
        fui_die 2 "$NEW_DIR exists and is not empty"
    fi
    local variant=$FUI_CLI/template/$NEW_TEMPLATE
    NEW_SIZE=$(tr -d '\n' < "$variant/size")
    mkdir -p "$NEW_DIR" || fui_die 1 "cannot create $NEW_DIR"
    new_write_tree "$FUI_CLI/template/common"
    new_write_tree "$variant"
    [ ! -f "$variant/source" ] || new_copy_source "$variant"
    fui_info "created $NEW_DIR from template '$NEW_TEMPLATE' (next: cd $NEW_DIR && fui build --targets native,web --verify)"
}
