import ResearchPanel from "./ResearchPanel";

type ResearchModuleProps = {
  onOpenWikiPage: (path: string) => void;
};

export default function ResearchModule({ onOpenWikiPage }: ResearchModuleProps) {
  return <ResearchPanel onOpenWikiPage={onOpenWikiPage} />;
}
