#!/usr/bin/perl
# tools/asm_norm.pl -- normalizes a firnc1 assembly text for tools/prune_compare.sh (r734).
#
# The static text constants (`.Lstatic_text.N:` + `.byte ...`) are numbered in the
# order the lowering meets them, and lazy lowering meets fewer of them (the ones
# of dead functions never exist) in another order. So each block is cut out and
# every REFERENCE to a label is replaced by the BYTES the label stands for:
# two texts are equal after this exactly when they use the same constants in the
# same places. Unreferenced constants vanish.
use strict;
use warnings;
local $/;
my $t = <STDIN>;
my %bytes;
while ($t =~ s/\.section \.rodata\n\s*\.p2align 0\n\.Lstatic_text\.(\d+):\n((?:[ \t]*\.byte [^\n]*\n)+)//) {
    ($bytes{$1} = $2) =~ s/\s+/ /g;
}
$t =~ s/\.Lstatic_text\.(\d+)/".Lstatic_text<" . (defined $bytes{$1} ? $bytes{$1} : "?") . ">"/ge;
# The data sections (one block per static) come out in the order the lowering
# first meets the static; sort them so that only the content counts. Code
# blocks (`.text`) stay where they are.
my @parts = split /(?=^\.section |^\.text)/m, $t;
my (@code, @data);
for my $p (@parts) {
    if ($p =~ /^\.text|^\.section \.text/) { push @code, $p } else { push @data, $p }
}
print @code, sort @data;
