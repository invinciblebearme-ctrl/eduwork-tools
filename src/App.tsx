import { WorkflowTools } from './components/WorkflowTools'
import { useState } from 'react'
import { ChevronRight, CircleHelp, Clock3, FolderOpen, Sparkles } from 'lucide-react'
import { PathPicker } from './components/PathPicker'
import { ImageTools } from './components/ImageTools'
import { PdfTools } from './components/PdfTools'
import { ExcelTools } from './components/ExcelTools'
import { Phase6Tools } from './components/Phase6Tools'
import { Sidebar } from './components/Sidebar'
import { SettingsTools } from './components/SettingsTools'
import type { PageId } from './types'

const titles: Record<PageId, string> = { home: '홈', pdf: 'PDF 도구', excel: 'Excel 도구', image: '이미지 도구', files: '파일 관리', convert: '문서 변환', privacy: '개인정보 점검', automation: '업무 자동화', settings: '설정' }
const phaseLabels: Record<PageId, string> = {
  home: 'EduWork Tools', pdf: 'Phase 4 · PDF 도구', excel: 'Phase 5 · Excel 도구',
  image: 'Phase 3 · 이미지 도구', files: 'Phase 2 · 파일 관리',
  convert: 'Phase 6 · 문서 작업', privacy: 'Phase 6 · 문서 작업',
  automation: 'Phase 7 · 업무 자동화', settings: '로컬 환경설정',
}
const footerLabels: Record<PageId, string> = {
  home: '홈', pdf: 'Phase 4', excel: 'Phase 5', image: 'Phase 3', files: 'Phase 2',
  convert: 'Phase 6', privacy: 'Phase 6', automation: 'Phase 7', settings: '설정',
}
export default function App() {
  const [active, setActive] = useState<PageId>('files')
  const phase = phaseLabels[active]
  return <div className="app-shell"><Sidebar active={active} onNavigate={setActive}/><main className="main-area"><header className="topbar"><div className="breadcrumbs"><span>EduWork Tools</span><ChevronRight size={14}/><strong>{titles[active]}</strong></div><div className="top-actions"><span className="today-chip"><Clock3 size={14}/>{phase}</span></div></header><div className="page-content">{active === 'settings' ? <SettingsTools/> : active === 'automation' ? <WorkflowTools/> : active === 'pdf' ? <PdfTools/> : active === 'excel' ? <ExcelTools/> : active === 'files' ? <><div className="page-intro"><div><div className="eyebrow"><FolderOpen size={14}/>로컬 파일 작업</div><h1>파일 관리</h1><p>파일을 선택해 미리 확인한 뒤 작업하세요. 파일 내용은 기기 밖으로 전송되지 않습니다.</p></div><span className="available-pill"><span/>사용 가능</span></div><PathPicker/></> : active === 'convert' ? <Phase6Tools mode='convert'/> : active === 'privacy' ? <Phase6Tools mode='privacy'/> : active === 'image' ? <ImageTools/> : active === 'home' ? <section className="coming-card"><Sparkles size={30}/><h2>EduWork Tools</h2><p>교육 업무를 위한 로컬 파일 도구</p><button className="hero-button" onClick={()=>setActive('files')}>파일 관리 열기 <ChevronRight size={15}/></button></section> : <section className="coming-card"><CircleHelp size={30}/><span className="status-pill">준비 중</span><h2>{titles[active]} 기능은 준비 중입니다</h2><p>현재 사용할 수 있는 기능은 PDF, 파일 관리, 이미지와 Excel 도구입니다.</p></section>}</div><footer className="app-footer"><span>EduWork Tools · {footerLabels[active]}</span><span>원본 보존 · 로컬 처리</span></footer></main></div>
}
