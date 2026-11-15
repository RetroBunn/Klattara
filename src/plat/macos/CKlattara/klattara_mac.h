/* Added to Klattara's C interface (klattara.h) for the Mac app only. */
#ifndef KLATTARA_MAC_H
#define KLATTARA_MAC_H

/* A voice's own value of a [:cv] setting such as "hs" or "fa", in the units
 * the command takes; NaN when the voice leaves it to the engine (F4 to F6 and
 * their bandwidths, unless set), or for an unknown voice or code. */
double klattara_mac_voice_setting(int index, const char *code);

#endif
