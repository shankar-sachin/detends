/* Symbol naming across object formats.
 *
 * Mach-O prefixes every C symbol with an underscore; ELF does not. Assembly has
 * to spell the decorated name out, so the difference is handled once here.
 *
 * Note that each directive must be on its own line. Apple's ARM assembler
 * treats `;` as the start of a comment rather than as a statement separator, so
 * a macro that folded `.globl`, `.p2align` and the label onto one line would
 * declare the symbol global and then quietly discard its definition — leaving
 * an undefined symbol that only shows up at link time. */
#ifndef DETENDS_ABI_H
#define DETENDS_ABI_H

#if defined(__APPLE__)
#define SYM(name) _##name
#else
#define SYM(name) name
#endif

#endif
