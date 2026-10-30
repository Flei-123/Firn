public class CaseDump {
    static boolean cased(int ch) {
        int type = Character.getType(ch);
        if (type == Character.LOWERCASE_LETTER || type == Character.UPPERCASE_LETTER || type == Character.TITLECASE_LETTER) return true;
        return (ch >= 0x02B0 && ch <= 0x02B8) || (ch >= 0x02C0 && ch <= 0x02C1) || (ch >= 0x02E0 && ch <= 0x02E4) || ch == 0x0345 || ch == 0x037A
            || (ch >= 0x1D2C && ch <= 0x1D61) || (ch >= 0x2160 && ch <= 0x217F) || (ch >= 0x24B6 && ch <= 0x24E9);
    }

    public static void main(String[] args) {
        StringBuilder b = new StringBuilder();
        int start = -1;
        for (int cp = 0; cp <= 0x110000; cp++) {
            boolean c = cp < 0x110000 && cased(cp);
            if (c && start < 0) start = cp;
            if (!c && start >= 0) {
                b.append("C ").append(start).append(' ').append(cp - 1).append('\n');
                start = -1;
            }
        }
        for (int cp = 0; cp < 0x110000; cp++) {
            int lo = Character.toLowerCase(cp);
            int up = Character.toUpperCase(cp);
            if (lo != cp) b.append("L ").append(cp).append(' ').append(lo).append('\n');
            if (up != cp) b.append("U ").append(cp).append(' ').append(up).append('\n');
        }
        System.out.print(b);
    }
}
