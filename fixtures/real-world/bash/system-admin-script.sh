#!/bin/bash
# LazyScripts System Administration Utility
# Complex bash script for real-world testing

LZS_VERSION=004
LZS_PREFIX=$(dirname $(readlink -f $BASH_SOURCE))
LZS_APP="$LZS_PREFIX/ls-init.sh"
LZS_URLPREFIX="git://github.com/hhoover/lazyscripts.git"

# Array declarations for system information
declare -a DISK_USAGE
declare -a NETWORK_INTERFACES
declare -A CONFIG_VARS

# Function to get system information
function get_system_info() {
    local hostname=$(hostname)
    local uptime=$(uptime | awk '{print $3,$4}' | sed 's/,//')
    local load_avg=$(cat /proc/loadavg | awk '{print $1,$2,$3}')

    echo "System: $hostname"
    echo "Uptime: $uptime"
    echo "Load Average: $load_avg"
}

# Function to check disk usage with error handling
function check_disk_usage() {
    local threshold=${1:-90}

    while IFS= read -r line; do
        DISK_USAGE+=("$line")
    done < <(df -h | grep -vE '^Filesystem|tmpfs|cdrom')

    for disk in "${DISK_USAGE[@]}"; do
        usage=$(echo $disk | awk '{print $5}' | sed 's/%//g')
        partition=$(echo $disk | awk '{print $1}')

        if [ $usage -gt $threshold ]; then
            echo "WARNING: $partition is ${usage}% full"
            return 1
        fi
    done

    return 0
}

# Function to manage MySQL operations
function mysql_operations() {
    local operation=$1
    local database=$2

    case $operation in
        "backup")
            if [[ -n $database ]]; then
                mysqldump --single-transaction $database > "${database}_$(date +%Y%m%d_%H%M%S).sql"
                echo "Backup completed for $database"
            else
                echo "Database name required for backup"
                return 1
            fi
            ;;
        "list_tables")
            mysql -e "USE $database; SHOW TABLES;" 2>/dev/null || {
                echo "Error: Cannot connect to database $database"
                return 1
            }
            ;;
        *)
            echo "Unknown operation: $operation"
            return 1
            ;;
    esac
}

# Function to process network interfaces
function network_info() {
    local interface_count=0

    # Read network interfaces into array
    while IFS= read -r interface; do
        if [[ $interface =~ ^[a-zA-Z] ]]; then
            NETWORK_INTERFACES+=("$interface")
            ((interface_count++))
        fi
    done < <(ip link show | grep -E '^[0-9]+:' | cut -d: -f2 | tr -d ' ')

    echo "Found $interface_count network interfaces:"
    for i in "${!NETWORK_INTERFACES[@]}"; do
        echo "  [$i] ${NETWORK_INTERFACES[$i]}"
    done
}

# Function with complex conditional logic
function service_manager() {
    local service_name=$1
    local action=$2

    # Check if systemd is available
    if command -v systemctl >/dev/null 2>&1; then
        case $action in
            "start"|"stop"|"restart"|"status")
                systemctl $action $service_name
                ;;
            "enable"|"disable")
                systemctl $action $service_name
                ;;
            *)
                echo "Invalid action: $action"
                return 1
                ;;
        esac
    elif command -v service >/dev/null 2>&1; then
        # Fallback to traditional service command
        service $service_name $action
    else
        echo "No service manager found"
        return 1
    fi
}

# Configuration management with associative array
function load_config() {
    local config_file=${1:-"/etc/lazyscripts.conf"}

    if [[ -f $config_file ]]; then
        while IFS='=' read -r key value; do
            # Skip comments and empty lines
            [[ $key =~ ^[[:space:]]*# ]] && continue
            [[ -z $key ]] && continue

            # Store in associative array
            CONFIG_VARS["$key"]="$value"
        done < "$config_file"

        echo "Loaded ${#CONFIG_VARS[@]} configuration variables"
    else
        echo "Configuration file not found: $config_file"
        return 1
    fi
}

# Main execution logic with argument parsing
function main() {
    local command=$1
    shift

    case $command in
        "sysinfo")
            get_system_info
            ;;
        "diskcheck")
            check_disk_usage "$@"
            ;;
        "netinfo")
            network_info
            ;;
        "mysql")
            mysql_operations "$@"
            ;;
        "service")
            service_manager "$@"
            ;;
        "config")
            load_config "$@"
            ;;
        "help"|*)
            echo "Usage: $0 {sysinfo|diskcheck|netinfo|mysql|service|config|help}"
            echo "  sysinfo          - Display system information"
            echo "  diskcheck [threshold] - Check disk usage"
            echo "  netinfo          - Show network interfaces"
            echo "  mysql <op> <db>  - MySQL operations"
            echo "  service <name> <action> - Service management"
            echo "  config [file]    - Load configuration"
            ;;
    esac
}

# Execute main function if script is run directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi