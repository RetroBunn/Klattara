# Reads "objdump -d" of an unstripped library and prints every instruction an
# ARMv5 phone cannot run: ARMv6/ARMv7-only instructions anywhere, and VFP
# (hardware floating point) instructions anywhere but libgcc's unwinder helpers
# that save and restore VFP registers, which run only when the unwind tables say
# a frame saved them, which soft-float ARMv5 code never does.
/^[0-9a-f]+ <.*>:/ { fn = $2; next }
{
	if ($0 ~ /[ \t](ldrex|strex|ldrexd|strexd|dmb|dsb|isb|movw|movt|rev|rev16|revsh|sxth|sxtb|uxth|uxtb|ubfx|sbfx|bfi|bfc|udiv|sdiv|cbz|cbnz|it|ite|itt)[ \t]/) {
		print fn ": " $0
	} else if ($0 ~ /[ \t](v[a-z]+(\.[a-z0-9]+)*|f(ld|st)m[a-z]*|fmrx|fmxr|fmrs|fmsr|fcpy[sd]|f(add|sub|mul|div|cmp)[sd])[ \t]/ && fn !~ /^<(__gnu_Unwind_(Save|Restore)_(VFP|VFP_D|VFP_D_16_to_31|WMMXD|WMMXC)|___Unwind_|__restore_core_regs)/) {
		print fn ": " $0
	}
}
