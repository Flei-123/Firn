# testdata/nbt

`bigtest.nbt.gz` -- the reference file of the NBT specification, Notch's own
example. Every NBT library in the world is checked against it.

* Origin: <https://raw.githubusercontent.com/twoolie/NBT/master/tests/bigtest.nbt>
  (the test data of the `NBT` package by Thomas Woolford), fetched
  2026-08-21. It is stored here exactly as it lies there: gzip'ed, 507
  octets, 1,544 octets unpacked.
* It is here so that `tools/nbt/run.sh` needs no network.
* What it is used for: `tools/nbt/bigtest.fi` rebuilds the same file with
  `lib/std/nbt.fi`, and the two are compared OCTET FOR OCTET over the whole
  1,543 octets of the original content (the 1,544th is its `TAG_End`; the
  Firn file goes on there with the three tags bigtest predates:
  `TAG_Int_Array`, `TAG_Long_Array` and an empty list).
