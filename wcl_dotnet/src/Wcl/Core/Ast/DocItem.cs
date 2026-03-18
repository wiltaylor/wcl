namespace Wcl.Core.Ast
{
    public abstract class DocItem { }

    public sealed class ImportItem : DocItem
    {
        public Import Import { get; }
        public ImportItem(Import import) => Import = import;
    }

    public sealed class ExportLetItem : DocItem
    {
        public ExportLet ExportLet { get; }
        public ExportLetItem(ExportLet exportLet) => ExportLet = exportLet;
    }

    public sealed class ReExportItem : DocItem
    {
        public ReExport ReExport { get; }
        public ReExportItem(ReExport reExport) => ReExport = reExport;
    }

    public sealed class FunctionDeclItem : DocItem
    {
        public FunctionDecl FunctionDecl { get; }
        public FunctionDeclItem(FunctionDecl functionDecl) => FunctionDecl = functionDecl;
    }

    public sealed class BodyDocItem : DocItem
    {
        public BodyItem BodyItem { get; }
        public BodyDocItem(BodyItem bodyItem) => BodyItem = bodyItem;
    }
}
