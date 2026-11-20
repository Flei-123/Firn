# SPDX-License-Identifier: MPL-2.0
TEMPLATE_DIR="$HERE/templates"
TEMPLATE_NAMES="cli lib web-api desktop-app game"

template_known() {
    case " $TEMPLATE_NAMES " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

template_list() {
    local t
    for t in $TEMPLATE_NAMES; do
        if [ -f "$TEMPLATE_DIR/$t/README.md" ]; then
            printf '  %-12s %s\n' "$t" "$(sed -n '3p' "$TEMPLATE_DIR/$t/README.md")"
        else
            printf '  %-12s %s\n' "$t" "fUi app for web, android, windows, linux (appkit)"
        fi
    done
}

template_check_name() {
    case $1 in [A-Za-z_]*) ;; *) die "name must start with a letter or _" ;; esac
    case $1 in *[!A-Za-z0-9_]*) die "name may only contain letters, digits and _" ;; esac
    [ -e "$1" ] && die "'$1' already exists"
    return 0
}

template_copy() {
    local tpl=$1 name=$2 src f rel dest
    src="$TEMPLATE_DIR/$tpl"
    [ -d "$src" ] || die "template '$tpl' is missing: $src"
    while IFS= read -r f; do
        rel=${f#"$src"/}
        dest="$name/${rel//__NAME__/$name}"
        mkdir -p "$(dirname "$dest")"
        sed "s/__NAME__/$name/g" "$f" > "$dest"
    done < <(find "$src" -type f | sort)
}

cmd_new_template() {
    local tpl=$1 name=${2:-}
    [ -n "$name" ] || die "usage: firn new TEMPLATE NAME   (templates: $TEMPLATE_NAMES)"
    template_check_name "$name"
    if [ "$tpl" = desktop-app ]; then
        cmd_new_app "$name"
        return $?
    fi
    template_copy "$tpl" "$name"
    case $tpl in
        lib) echo "created $name/ (lib) -- cd $name && firn test" ;;
        *)   echo "created $name/ ($tpl) -- cd $name && firn build . -o $name && firn test" ;;
    esac
}
