tpl_args() {
    case "$1" in
        settings) echo "--theme $2" ;;
        settings_list) echo "--theme $2 --size ${3}x${4}" ;;
        settings_page) echo "--theme $2 --size ${3}x${4} --page 0" ;;
        store) echo "--theme $2" ;;
        store_detail) echo "--theme $2 --detail 1" ;;
        store_phone) echo "--theme $2 --size ${3}x${4}" ;;
        files) echo "--theme $2" ;;
        files_grid) echo "--theme $2 --grid 1" ;;
        wizard_welcome) echo "--theme $2 --step 0" ;;
        wizard_region) echo "--theme $2 --step 1" ;;
        wizard_region_m) echo "--theme $2 --step 1 --size ${3}x${4}" ;;
    esac
}
