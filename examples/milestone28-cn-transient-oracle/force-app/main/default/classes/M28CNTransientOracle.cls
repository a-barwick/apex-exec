public class M28CNTransientOracle {
    public transient String hidden { get; set; }
    public String visible { get; set; }

    public static void run() {
        M28CNTransientOracle value = new M28CNTransientOracle();
        value.hidden = 'hidden';
        value.visible = 'visible';
        System.debug('APEX_EXEC_ORACLE_VALUE|hidden|' + value.hidden);
        System.debug('APEX_EXEC_ORACLE_VALUE|visible|' + value.visible);
        System.debug('APEX_EXEC_ORACLE_VALUE|json|' + JSON.serialize(value));
    }
}
