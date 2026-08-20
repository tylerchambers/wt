use crate::cli::Shell;

const HOOK: &str = r#"unalias wt 2>/dev/null || true
function wt() {
    if [ "$#" -gt 0 ] && [ "$1" = "new" ]; then
        local wt_arg wt_has_cd=0 wt_has_conflict=0 wt_option_mode=1 wt_skip_command=1
        local wt_path wt_status
        local -a wt_args=("new" "--print-path")

        for wt_arg in "$@"; do
            if [ "$wt_skip_command" -eq 1 ]; then
                wt_skip_command=0
                continue
            fi
            if [ "$wt_option_mode" -eq 1 ]; then
                case "$wt_arg" in
                    --)
                        wt_option_mode=0
                        wt_args+=("$wt_arg")
                        ;;
                    --cd)
                        wt_has_cd=1
                        ;;
                    --json|--print-path)
                        wt_has_conflict=1
                        wt_args+=("$wt_arg")
                        ;;
                    *)
                        wt_args+=("$wt_arg")
                        ;;
                esac
            else
                wt_args+=("$wt_arg")
            fi
        done

        if [ "$wt_has_cd" -eq 1 ]; then
            if [ "$wt_has_conflict" -eq 1 ]; then
                printf '%s\n' 'error: --cd cannot be used with --json or --print-path' >&2
                return 2
            fi

            wt_path="$(command wt "${wt_args[@]}")"
            wt_status=$?
            if [ "$wt_status" -ne 0 ]; then
                return "$wt_status"
            fi
            builtin cd -- "$wt_path"
            return $?
        fi
    fi

    command wt "$@"
}
"#;

pub fn init(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash | Shell::Zsh => HOOK,
    }
}
