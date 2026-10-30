import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'

type Template = { name: string; kind: string; format: string; width: number | null; groupColumn: number | null }
type Run = { template: Template; input: string; destination: string; startedAt: string; finishedAt: string; fileCount: number; outputs: string[]; success: boolean; errors: string[]; skippedLinks: number }
type Store = { templates: Template[]; history: Run[] }
const initial: Template = { name: '', kind: 'images', format: 'png', width: null, groupColumn: null }
export function WorkflowTools() {
  const [store, setStore] = useState<Store>({ templates: [], history: [] })
  const [template, setTemplate] = useState(initial)
  const [input, setInput] = useState('')
  const [destination, setDestination] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [loaded, setLoaded] = useState(false)
  async function load() { setStore(await invoke<Store>('workflow_load')); setLoaded(true) }
  useEffect(() => { void load().catch(e => setError(String(e))) }, [])
  async function action(job: () => Promise<void>) { setBusy(true); setError(''); try { await job() } catch (e) { setError(String(e)) } finally { setBusy(false) } }
  async function select(output: boolean) {
    await action(async () => {
      const path = output && template.kind !== 'images'
        ? await save({ title: '새 출력 파일 선택', filters: [{ name: template.kind === 'pdf' ? 'PDF' : 'Excel', extensions: [template.kind === 'pdf' ? 'pdf' : 'xlsx'] }] })
        : await open({ directory: true, multiple: false, title: output ? '출력 폴더 선택' : '입력 폴더 선택' })
      if (typeof path === 'string') { if (output) setDestination(path); else setInput(path) }
    })
  }
  return <div className="file-tools"><div className="page-intro"><div><h1>업무 자동화</h1><p>이름 있는 템플릿과 실행 기록을 이 기기에 저장합니다. 실행마다 입력 폴더와 출력 위치를 선택하세요.</p></div></div>
    <section className="operation-card"><h3>워크플로 템플릿</h3><fieldset disabled={busy || !loaded} style={{ border: 0, padding: 0 }}>
      <div className="form-grid"><label>저장된 템플릿<select value="" onChange={e => { const t = store.templates.find(t => t.name === e.target.value); if (t) { setTemplate(t); setDestination('') } }}><option value="">불러오기</option>{store.templates.map(t => <option key={t.name}>{t.name}</option>)}</select></label>
      <label>템플릿 이름<input value={template.name} onChange={e => setTemplate({ ...template, name: e.target.value })}/></label>
      <label>작업<select value={template.kind} onChange={e => { setTemplate({ ...template, kind: e.target.value }); setDestination('') }}><option value="images">폴더 이미지 → 크기 조정·변환</option><option value="pdf">폴더 PDF → 파일명순 병합</option><option value="excel">호환 XLSX → 명단 병합·중복/빈 행 정리</option></select></label>
      {template.kind === 'images' && <><label>출력 형식<select value={template.format} onChange={e => setTemplate({ ...template, format: e.target.value })}><option value="png">PNG</option><option value="jpeg">JPEG</option><option value="webp">WebP</option></select></label><label>너비 px (비우면 원래 크기, 비율 유지)<input type="number" min={1} max={30000} value={template.width ?? ''} onChange={e => setTemplate({ ...template, width: e.target.value ? Number(e.target.value) : null })}/></label></>}
      {template.kind === 'excel' && <label>그룹 열 번호 (1부터, 선택 사항)<input type="number" min={1} max={16384} value={template.groupColumn ?? ''} onChange={e => setTemplate({ ...template, groupColumn: e.target.value ? Number(e.target.value) : null })}/></label>}</div>
      <p>같은 이름으로 저장하면 템플릿을 갱신합니다. XLSX는 시트 이름·머리글이 같은 단일 시트 2개 이상을 사용하며 문자열 값으로 저장합니다. 그룹은 값순으로 group-001부터 저장합니다.</p>
      <div className="operation-actions"><button className="button button-secondary" onClick={() => void action(async () => setStore(await invoke<Store>('workflow_save', { template, delete: false })))}>템플릿 저장</button><button className="button button-secondary" disabled={!store.templates.some(t => t.name === template.name)} onClick={() => void action(async () => setStore(await invoke<Store>('workflow_save', { template, delete: true })))}>템플릿 삭제</button></div>
      <div className="operation-actions"><button className="button button-secondary" onClick={() => void select(false)}>입력 폴더 선택</button><span>{input || '미선택'}</span></div>
      <div className="operation-actions"><button className="button button-secondary" onClick={() => void select(true)}>출력 위치 선택</button><span>{destination || '미선택'}</span></div>
      <p>폴더 바로 아래 최대 50파일 · 파일당 100 MiB · 합계 250 MiB. 심볼릭 링크는 건너뜁니다. 기존 출력은 덮어쓰지 않습니다. 이미지는 파일별 계속 처리하고 PDF·Excel은 오류 시 실패합니다.</p>
      <button className="button button-primary" disabled={!input || !destination || !template.name.trim()} onClick={() => void action(async () => { await invoke<Run>('workflow_run', { template, input, destination }); await load() })}>실행</button>
    </fieldset>{busy && <p role="status">처리 중입니다. 취소와 백분율 진행률은 지원하지 않습니다. 완료될 때까지 기다려 주세요.</p>}{error && <p className="error-message" role="alert">{error}</p>}{!loaded && !busy && <button className="button button-secondary" onClick={() => void action(load)}>저장 파일 다시 읽기</button>}</section>
    <section className="operation-card"><h3>실행 기록 (최근 200회)</h3>{[...store.history].reverse().map((r, i) => <details key={`${r.startedAt}-${i}`}><summary>{r.template.name} · {r.finishedAt ? r.success ? '성공' : '실패' : '완료 기록 없음'} · {r.fileCount}파일 · {new Date(r.startedAt).toLocaleString()}</summary><p>시작 {r.startedAt} / 종료 {r.finishedAt || '기록 없음 (앱 종료 등)'}</p><p>입력: {r.input}<br/>출력 지정: {r.destination}<br/>건너뛴 링크: {r.skippedLinks}</p><p>완료된 출력 {r.outputs.length}개{!r.success && r.outputs.length > 0 ? " · 일부만 완료되었습니다. 아래 파일을 확인한 뒤 재실행하세요." : ""}</p>{r.outputs.map(p => <p key={p}>{p}</p>)}{r.errors.map((e,j) => <p className="error-message" key={j}>{e}</p>)}</details>)}</section>
    <section className="operation-card"><h3>지원되는 자동화 작업</h3><p>현재 템플릿으로 실행할 수 있는 것은 폴더 이미지 크기 조정·변환, PDF 파일명순 병합, 호환 XLSX 단일 시트 병합 및 그룹 분리입니다. 문서 PDF 변환과 개인정보 점검은 각각 문서 변환·개인정보 점검 메뉴에서 실행합니다. 여기서 자동화 템플릿으로 연계되지는 않습니다.</p></section>
  </div>
}
