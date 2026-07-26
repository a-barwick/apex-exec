public class M28CN5IntegerValueOfOracle {
    public static void run() {
        System.debug('APEX_EXEC_ORACLE_VALUE|stringValue|' + Integer.valueOf('42'));
        System.debug('APEX_EXEC_ORACLE_VALUE|signedString|' + Integer.valueOf('-7'));
        System.debug('APEX_EXEC_ORACLE_VALUE|integerValue|' + Integer.valueOf(19));
        System.debug('APEX_EXEC_ORACLE_VALUE|integerNull|' + (Integer.valueOf((Integer) null) == null));
        try {
            Integer.valueOf('42.5');
            System.debug('APEX_EXEC_ORACLE_VALUE|invalidStringThrows|false');
        } catch (TypeException error) {
            System.debug('APEX_EXEC_ORACLE_VALUE|invalidStringThrows|true');
        }
        try {
            Integer.valueOf('2147483648');
            System.debug('APEX_EXEC_ORACLE_VALUE|overflowStringThrows|false');
        } catch (TypeException error) {
            System.debug('APEX_EXEC_ORACLE_VALUE|overflowStringThrows|true');
        }
        try {
            Integer.valueOf((String) null);
            System.debug('APEX_EXEC_ORACLE_VALUE|stringNullThrows|false');
        } catch (NullPointerException error) {
            System.debug('APEX_EXEC_ORACLE_VALUE|stringNullThrows|true');
        }
    }
}
