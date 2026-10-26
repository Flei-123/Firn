import ctypes
import sys
import numpy as np

LIB = ctypes.CDLL('libjxl.so.0.7')
LIB.JxlDecoderCreate.restype = ctypes.c_void_p
LIB.JxlDecoderCreate.argtypes = [ctypes.c_void_p]
for name in ('JxlDecoderSubscribeEvents', 'JxlDecoderSetInput', 'JxlDecoderProcessInput',
             'JxlDecoderGetBasicInfo', 'JxlDecoderImageOutBufferSize', 'JxlDecoderSetImageOutBuffer'):
    getattr(LIB, name).restype = ctypes.c_int


class PixelFormat(ctypes.Structure):
    _fields_ = [('num_channels', ctypes.c_uint32), ('data_type', ctypes.c_int),
                ('endianness', ctypes.c_int), ('align', ctypes.c_size_t)]


TYPE_FLOAT = 0
TYPE_UINT8 = 2
TYPE_UINT16 = 3
EV_BASIC_INFO = 0x40
EV_FULL_IMAGE = 0x1000
ST_NEED_OUT = 5


def decode(path, data_type=TYPE_FLOAT, channels=3):
    data = open(path, 'rb').read()
    dec = LIB.JxlDecoderCreate(None)
    assert LIB.JxlDecoderSubscribeEvents(ctypes.c_void_p(dec), EV_BASIC_INFO | EV_FULL_IMAGE) == 0
    buf = ctypes.create_string_buffer(data, len(data))
    assert LIB.JxlDecoderSetInput(ctypes.c_void_p(dec), buf, ctypes.c_size_t(len(data))) == 0
    info = ctypes.create_string_buffer(512)
    fmt = PixelFormat(channels, data_type, 0, 0)
    out = None
    w = h = 0
    while True:
        st = LIB.JxlDecoderProcessInput(ctypes.c_void_p(dec))
        if st == EV_BASIC_INFO:
            assert LIB.JxlDecoderGetBasicInfo(ctypes.c_void_p(dec), info) == 0
            w = int.from_bytes(info.raw[4:8], 'little')
            h = int.from_bytes(info.raw[8:12], 'little')
        elif st == ST_NEED_OUT:
            size = ctypes.c_size_t(0)
            assert LIB.JxlDecoderImageOutBufferSize(ctypes.c_void_p(dec), ctypes.byref(fmt), ctypes.byref(size)) == 0
            out = ctypes.create_string_buffer(size.value)
            assert LIB.JxlDecoderSetImageOutBuffer(ctypes.c_void_p(dec), ctypes.byref(fmt), out, size) == 0
        elif st == EV_FULL_IMAGE:
            break
        elif st == 0:
            break
        else:
            raise RuntimeError('status %d' % st)
    dt = {TYPE_FLOAT: np.float32, TYPE_UINT8: np.uint8, TYPE_UINT16: np.uint16}[data_type]
    return np.frombuffer(out.raw, dtype=dt).reshape(h, w, channels).copy()


if __name__ == '__main__':
    a = decode(sys.argv[1], TYPE_FLOAT)
    np.save(sys.argv[2], a)
    print(a.shape, a.min(), a.max())
