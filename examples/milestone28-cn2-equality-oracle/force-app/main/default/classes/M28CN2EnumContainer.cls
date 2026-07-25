public class M28CN2EnumContainer {
    public enum SourceMetadataType { AnonymousBlock, ApexClass }

    public class SourceMetadata {
        public SourceMetadataType MetadataType { get; private set; }

        public SourceMetadata() {
            MetadataType = SourceMetadataType.ApexClass;
        }
    }

    public SourceMetadata Source { get; private set; }

    public M28CN2EnumContainer() {
        Source = new SourceMetadata();
    }
}
