import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { FileImage, FolderOpen, ImagePlus, Play, Trash2 } from 'lucide-react'
import type { SelectedPath } from '../types'

type Format = 'jpeg' | 'png' | 'webp'
type Result = { source: string; output: string | null; beforeBytes: number | null; afterBytes: number | null; error: string | null }
const errorText = (e: unknown) => e instanceof Error ? e.message : String(e)
export function ImageTools() {
  const [files, setFiles] = useState<SelectedPath[]>([])
  const [format, setFormat] = useState<Format>('webp')
  const [width, setWidth] = useState(''); const [height, setHeight] = useState('')
  const [exact, setExact] = useState(false); const [squareCanvas, setSquareCanvas] = useState(false)
  const [quality, setQuality] = useState(85); const [outputDir, setOutputDir] = useState('')
  const [results, setResults] = useState<Result[]>([]); const [busy, setBusy] = useState(false); const [error, setError] = useState('')
  async function addFiles() {
    try {
    const selected = await open({ multiple: true, directory: false, title: '이미지 선택', filters: [{ name: '지원 이미지', extensions: ['jpg', 'jpeg', 'png', 'webp'] }] })
    if (!selected) return
    const paths = Array.isArray(selected) ? selected : [selected]
    setFiles(current => [...current, ...paths.filter(p => !current.some(f => f.path === p)).map(path => ({ path, kind: 'file' as const }))])
    } catch (e) { setError(`파일 선택 창을 열지 못했습니다. ${errorText(e)}`) }
  }
  async function chooseOutput() { try { const selected = await open({ directory: true, multiple: false, title: '저장 폴더 선택' }); if (typeof selected === 'string') setOutputDir(selected) } catch (e) { setError(`폴더 선택 창을 열지 못했습니다. ${errorText(e)}`) } }
  async function convert() {
    if (!files.length || !outputDir) return
    setBusy(true); setError(''); setResults([])
    try { setResults(await invoke<Result[]>('convert_images', { paths: files.map(f => f.path), outputDir, options: { format, width: width ? Number(width) : null, height: height ? Number(height) : null, exact, squareCanvas, quality } })) }
    catch (e) { setError(String(e)) } finally { setBusy(false) }
  }
  const bytes = (n: number | null) => n == null ? '—' : `${n.toLocaleString()} B`
  return <div className="image-tools">
    <div className="page-intro"><div><div className="eyebrow"><FileImage size={14}/>기기에서 처리</div><h1>이미지 도구</h1><p>이미지를 일괄 변환하고 크기를 조정합니다. 원본은 그대로 보존됩니다.</p></div><span className="available-pill"><span/>사용 가능</span></div>
    <section className="operation-card"><h3>1. 이미지 선택</h3><p>JPG, JPEG, PNG, WebP 래스터 이미지 파일을 선택하세요.</p><div className="picker-actions"><button className="button button-primary" onClick={() => void addFiles()}><ImagePlus size={15}/>이미지 추가</button><button className="button button-secondary" onClick={() => setFiles([])} disabled={!files.length}><Trash2 size={15}/>목록 비우기</button><span className="count-badge">{files.length}개</span></div>
      {files.length > 0 && <div className="image-file-list">{files.map(f=><div key={f.path}><FileImage size={15}/><span title={f.path}>{f.path.split(/[\\/]/).at(-1)}</span><button aria-label="삭제" onClick={()=>setFiles(current=>current.filter(x=>x.path!==f.path))}><Trash2 size={14}/></button></div>)}</div>}</section>
    <section className="operation-card"><h3>2. 변환 설정</h3><p>비율 유지 시 지정한 상자 안에 들어가도록 크기를 조정합니다. 정확한 크기는 이미지를 늘리거나 줄여 맞춥니다.</p>
      <div className="form-grid"><label>출력 형식<select value={format} onChange={e=>setFormat(e.target.value as Format)}><option value="jpeg">JPEG</option><option value="png">PNG</option><option value="webp">WebP (손실)</option></select></label><label>품질: {quality}<input type="range" min="1" max="100" value={quality} onChange={e=>setQuality(Number(e.target.value))} disabled={format==='png'}/></label><label>너비 (px, 선택)<input type="number" min="1" max="30000" value={width} onChange={e=>setWidth(e.target.value)}/></label><label>높이 (px, 선택)<input type="number" min="1" max="30000" value={height} onChange={e=>setHeight(e.target.value)}/></label></div>
      <label className="check-line"><input type="checkbox" checked={exact} onChange={e=>setExact(e.target.checked)}/> 너비와 높이에 정확히 맞추기</label><label className="check-line"><input type="checkbox" checked={squareCanvas} onChange={e=>setSquareCanvas(e.target.checked)}/> 비율을 유지해 정사각형 캔버스에 가운데 배치 (PNG/WebP는 투명, JPEG는 흰색 배경)</label>
    </section>
    <section className="operation-card"><h3>3. 저장 위치 및 실행</h3><div className="picker-actions"><button className="button button-secondary" onClick={()=>void chooseOutput()}><FolderOpen size={15}/>저장 폴더 선택</button><span className="selected-folder" title={outputDir}>{outputDir || '선택된 폴더 없음'}</span></div><p className="image-note">이미지 한 장은 최대 40MP(4천만 픽셀), 가로와 세로는 각각 최대 30,000px까지 처리합니다. 기존 파일은 덮어쓰지 않습니다. 같은 이름이 있으면 새 이름을 사용하며 파일별 오류가 있어도 나머지 변환은 계속됩니다. 변환 취소 기능은 제공하지 않습니다.</p><button className="button button-primary" disabled={busy || !files.length || !outputDir} onClick={()=>void convert()}><Play size={15}/>{busy?'변환 중…':'일괄 변환'}</button>{error&&<p className="error-message" role="alert">{error}</p>}</section>
    {results.length>0&&<section className="operation-card"><h3>변환 결과</h3><p>{results.filter(r=>!r.error).length}개 성공 · {results.filter(r=>r.error).length}개 실패</p><div className="image-results">{results.map((r,i)=><div className={r.error?'image-result failed':'image-result'} key={`${r.source}-${i}`}><div className="image-result-name">{r.error?'⚠':'✓'} <span title={r.source}>{r.source.split(/[\\/]/).at(-1)}</span></div><div className="image-result-detail">{r.error ? r.error : <><span>{bytes(r.beforeBytes)} → {bytes(r.afterBytes)}</span><span title={r.output??''}>{r.output}</span></>}</div></div>)}</div></section>}
  </div>
}
