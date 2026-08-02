# Shilpo Fish Shell Configuration Fragment
if status is-interactive
    if type -q starship
        starship init fish | source
    end
end

fish_add_path ~/.local/bin

if type -q paru; and not type -q yay
    alias yay paru
end
