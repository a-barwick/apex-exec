public class M28CN2EqualityOracle {
    public static void run() {
        M28CN2EnumContainer value = new M28CN2EnumContainer();
        System.debug('APEX_EXEC_ORACLE_VALUE|same|' +
            (value.Source.MetadataType == M28CN2EnumContainer.SourceMetadataType.ApexClass));
        System.debug('APEX_EXEC_ORACLE_VALUE|different|' +
            (value.Source.MetadataType != M28CN2EnumContainer.SourceMetadataType.AnonymousBlock));
    }
}
