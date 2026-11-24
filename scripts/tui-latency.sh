#!/usr/bin/env bash
# Times how long the TUI takes to show its own writes (spec F28), by driving
# target/release/asn on a throwaway board in a Rex pane and reading its screen.
#
#   scripts/tui-latency.sh <project-gid> [watch-seconds]
#
# Run it from a Rex terminal (it needs REX_SESSION), after
# `cargo build --release`, with asn's usual config. ASN=/path/to/asn measures
# another binary (say, the Homebrew release); LIMIT=<seconds> changes how long
# a check waits (default 300). The board's first section must be selected
# when it opens (it is, unless a "(no section)" column shows).
#
# It creates one task, named "asn latency probe <time>", then moves it to the
# second section, assigns it to you, sets the board's first single-select
# field (if there is one), edits its description, comments on it, and
# completes it. After each write it keeps watching for watch-seconds (default
# 120) to catch the change flicking back. It prints, per write, how long the
# change took to show after the last key, and whether it flicked back.
# Before each key that acts on the task, it checks the task is the one
# selected, and it stops at the first check that times out, so it never acts
# on another task. If it stops early, the task stays open; the latency
# example completes it on its next run.
set -euo pipefail

usage="usage: scripts/tui-latency.sh <project-gid> [watch-seconds]"
project=${1:?$usage}
watch=${2:-120}
session=${REX_SESSION:?run this from a Rex terminal (REX_SESSION is unset)}
root=$(cd "$(dirname "$0")/.." && pwd)
asn=${ASN:-$root/target/release/asn}
limit=${LIMIT:-300} # seconds before a check gives up

[[ -x $asn ]] || { echo "no $asn: run cargo build --release" >&2; exit 1; }
built=$(stat -f %Sm "$asn" 2>/dev/null || stat -c %y "$asn")
echo "binary: $("$asn" --version), built $built"

sections=$("$asn" project sections "$project" 2>/dev/null)
first=$(sed -n 1p <<<"$sections" | cut -f2)
second=$(sed -n 2p <<<"$sections" | cut -f2)
[[ -n $second ]] || { echo "the board needs at least two sections" >&2; exit 1; }
# Fields are unindented "gid<TAB>name" lines; their options are indented.
field=$("$asn" project fields "$project" 2>/dev/null | grep -v '^[[:space:]]' | sed -n 1p | cut -f2 || true)
name="asn latency probe $(date -u +%H%M%S)"

# Checks the screen on stdin. Exits 0 when the check holds. The board area is
# the screen without its title line and its status and hint lines.
#   loaded -              the board has loaded (its title counts open tasks)
#   text STRING           the screen contains STRING
#   area STRING           the board area contains STRING, and no editor or
#                         picker is open
#   gone STRING           the board area doesn't contain STRING
#   in COLUMN STRING      STRING is a card in the column titled COLUMN
#   assigned STRING       the card STRING shows an assignee (not "—")
#   selected COLUMN       the column titled COLUMN is the selected one
#   cursor STRING         the selected card is STRING
read -r -d '' check_pl <<'PERL' || true
use strict;
use warnings;
use utf8;
my ($mode, $arg, $text) = @ARGV;
my @lines = <STDIN>;
chomp @lines;
my @area = @lines[1 .. ($#lines > 2 ? $#lines - 2 : 0)];
my $screen = join("\n", @lines);
my $in_area = grep { index($_, $arg) >= 0 } @area;
# The columns: the header line's titles, and each row's cells, both split
# at the box-drawing borders, so wide characters don't shift them.
my ($header) = grep { /^[┏┌]/ } @area;
my @titles = map { /^\s*(.*?)\s*\(\d+\)/ ? $1 : '' } split /[┏┓┌┐]+/, ($header // '');
my @rows = map { [split /[┃│]+/] } grep { /^[┃│]/ } @area;
sub column { my ($title) = @_; my ($i) = grep { $titles[$_] eq $title } 0 .. $#titles; $i }
my $ok = 0;
if ($mode eq 'loaded') {
    $ok = ($lines[0] // '') =~ /· \d+ open/;
} elsif ($mode eq 'text') {
    $ok = index($screen, $arg) >= 0;
} elsif ($mode eq 'area') {
    $ok = $in_area && index($screen, 'Esc cancel') < 0;
} elsif ($mode eq 'gone') {
    $ok = !$in_area;
} elsif ($mode eq 'in') {
    my $i = column($arg);
    $ok = defined $i && grep { index($_->[$i] // '', $text) >= 0 } @rows;
} elsif ($mode eq 'assigned') {
    ROW: for my $r (0 .. $#rows - 1) {
        for my $i (0 .. $#{ $rows[$r] }) {
            next unless index($rows[$r][$i], $arg) >= 0;
            my $below = $rows[$r + 1][$i] // '';
            $ok = $below =~ /\S/ && $below !~ /^\s*—/;
            last ROW;
        }
    }
} elsif ($mode eq 'cursor') {
    $ok = grep { /▌\Q$arg\E/ } @area;
} elsif ($mode eq 'selected') {
    $ok = defined $header && $header =~ /┏ \Q$arg\E \(/;
}
exit($ok ? 0 : 1);
PERL

block=$(rex run -s "$session" --label asn-latency --focus=false --json -c "$root" -- \
    "$asn" --project "$project" | grep -o '"block:[^"]*"' | head -1 | tr -d '"')
[[ -n $block ]] || { echo "rex didn't start asn" >&2; exit 1; }

screen() { rex capture -s "$session" -b "$block" --trim 2>/dev/null; }
keys() { rex send-key -s "$session" -b "$block" "$@"; }
type_text() { rex send -s "$session" -b "$block" "$1"; }
now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000'; }
check() { screen | perl -CSA -e "$check_pl" -- "$@"; }

quit() {
    keys escape escape 2>/dev/null || true
    type_text ':q' 2>/dev/null && keys enter 2>/dev/null || true
}
trap quit EXIT

# Waits (untimed, up to 30 s) for a state of the UI before the next key.
ready() {
    local end=$(($(now_ms) + 30000))
    until check "$@"; do
        (($(now_ms) < end)) || { echo "gave up waiting for: $*" >&2; exit 1; }
        sleep 0.2
    done
}

# Prints the milliseconds since $t0 until the check holds, or >limit.
wait_for() {
    until check "$@"; do
        (($(now_ms) - t0 < limit * 1000)) || { echo ">$limit"; return; }
        sleep 0.2
    done
    echo $(($(now_ms) - t0))
}

# Watches for $watch seconds; prints "yes" if the check stops holding on two
# captures in a row, and keeps the screen that showed it in $TMPDIR.
stays() {
    local end=$(($(now_ms) + watch * 1000)) seen
    while (($(now_ms) < end)); do
        if ! check "$@"; then
            sleep 0.3
            seen=$(screen)
            if ! perl -CSA -e "$check_pl" -- "$@" <<<"$seen"; then
                local kept="${TMPDIR:-/tmp}/tui-latency-$1-$(date +%s).txt"
                printf '%s\n' "$seen" >"$kept"
                echo "flicked back ($*); screen kept in $kept" >&2
                echo yes
                return
            fi
        fi
        sleep 1
    done
    echo no
}

# Times one write: the label, then the check that shows it. Set t0 just
# before sending the write's last key. Stops the run at a timeout: what
# follows would act on whatever card is selected.
measure() {
    local label=$1 ms reverted=n/a
    shift
    ms=$(wait_for "$@")
    [[ $ms == ">"* ]] || reverted=$(stays "$@")
    printf '%-12s %10s %9s\n' "$label" "$ms" "$reverted"
    [[ $ms != ">"* ]] || { echo "stopping: $label didn't show; the task stays open" >&2; exit 1; }
}

ready loaded -
check selected "$first" || { echo "the first section ($first) isn't selected" >&2; exit 1; }
printf '%-12s %10s %9s\n' write visible-ms reverted

keys tab q
ready text 'NEW TASK'
type_text "$name"
t0=$(now_ms); keys enter
measure create in "$first" "$name"

ready cursor "$name"
keys m
ready text "✓ $first"
keys ctrl+n
ready text "▌$second"
t0=$(now_ms); keys enter
measure move in "$second" "$name"

ready cursor "$name"
keys tab a
ready text '▌Me'
t0=$(now_ms); keys enter
measure assign assigned "$name"

ready cursor "$name"
keys enter
ready text "┃ $name"
if [[ -n $field ]]; then
    type_text F
    ready text ' Set '
    if check text ' Set field '; then keys enter; fi
    ready text '(none)'
    keys ctrl+n
    option=$(screen | perl -CS -Mutf8 -ne 'if (/▌(?:✓ )?(.+?)\s*┃/) { print $1; exit }')
    t0=$(now_ms); keys enter
    measure field area "$field: $option"
else
    printf '%-12s %10s %9s\n' field n/a n/a
fi

keys tab e
ready text 'Description of'
type_text "latency check $name"
t0=$(now_ms); keys ctrl+s
measure description area "latency check $name"

keys c
ready text 'Comment on'
type_text "latency comment $name"
t0=$(now_ms); keys ctrl+s
measure comment area "latency comment $name"

keys escape
ready cursor "$name"
t0=$(now_ms); keys d
measure complete gone "$name"
