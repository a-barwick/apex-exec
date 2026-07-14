List<String> strs = new List<String>();

for (Integer i = 0; i < 100; i++) {
    String s = String.valueOf(i);
    strs.add(s);
}

System.debug(String.join(strs, ''));
