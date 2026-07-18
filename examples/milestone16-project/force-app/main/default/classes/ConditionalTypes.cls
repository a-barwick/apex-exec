public class ConditionalTypes {
    public static String describe(Boolean primary, Object value) {
        String label = primary ? 'primary' : 'secondary';
        return label + ':' + (value instanceof String ? 'String' : 'Other');
    }

    public static String run() {
        String result = describe(true, 'value')
            + '|' + describe(false, new List<String>{'value'});
        System.debug(result);
        return result;
    }
}
