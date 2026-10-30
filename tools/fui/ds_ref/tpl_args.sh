tpl_args() {
    case "$1" in
        settings) echo "--theme $2" ;;
        settings_list) echo "--theme $2 --size ${3}x${4}" ;;
        settings_page) echo "--theme $2 --size ${3}x${4} --page 0" ;;
        wizard_welcome) echo "--theme $2 --step 0" ;;
        wizard_region) echo "--theme $2 --step 1" ;;
    esac
}
