Android's C library symbol maps (bionic, tag android-11.0.0_r1), from
https://android.googlesource.com/platform/bionic, under the Apache License 2.0.
They record which Android version introduced each function, back to 4.0 (API 14),
which today's maps no longer do. check_old_android.py uses them to prove that
libklattara.so needs nothing a device's Android does not have.
