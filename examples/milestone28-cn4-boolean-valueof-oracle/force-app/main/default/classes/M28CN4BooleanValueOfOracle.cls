public class M28CN4BooleanValueOfOracle {
    public static void run() {
        System.debug('APEX_EXEC_ORACLE_VALUE|lowercaseTrue|' + Boolean.valueOf('true'));
        System.debug('APEX_EXEC_ORACLE_VALUE|uppercaseTrue|' + Boolean.valueOf('TRUE'));
        System.debug('APEX_EXEC_ORACLE_VALUE|falseValue|' + Boolean.valueOf('false'));
        System.debug('APEX_EXEC_ORACLE_VALUE|otherValue|' + Boolean.valueOf('yes'));
        System.debug('APEX_EXEC_ORACLE_VALUE|spacePadded|' + Boolean.valueOf(' true '));
        try {
            Boolean.valueOf(null);
            System.debug('APEX_EXEC_ORACLE_VALUE|nullThrows|false');
        } catch (Exception error) {
            System.debug('APEX_EXEC_ORACLE_VALUE|nullThrows|true');
        }
    }
}
