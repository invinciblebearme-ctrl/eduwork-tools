import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import { ArrowDown, ArrowUp, FileImage, FilePlus2, Files, FolderOpen, Play, Trash2 } from 'lucide-react'

type Result = { output: string; fileCount: number; pageCount: number }
type Mode = 'merge' | 'extract' | 'split' | 'images' | 'parity' | 'rotate' | 'delete' | 'reorder'
const errorText = (e: unknown) => e instanceof Error ? e.message : String(e)
const basename = (p: string) => p.split(/[\\/]/).at(-1) || p
export function PdfTools() {
  const [mode,setMode]=useState<Mode>('merge'), [files,setFiles]=useState<string[]>([]), [source,setSource]=useState(''), [ranges,setRanges]=useState('1-3'), [parityOdd,setParityOdd]=useState(true), [degrees,setDegrees]=useState(90), [width,setWidth]=useState('210'), [height,setHeight]=useState('297'), [margin,setMargin]=useState('10'), [busy,setBusy]=useState(false), [error,setError]=useState(''), [result,setResult]=useState<Result|null>(null)
  async function pick(multiple:boolean, images=false) { try { const v=await open({multiple,directory:false,title:images?'이미지 선택':multiple?'병합할 PDF 선택':'PDF 선택',filters:[{name:images?'지원 이미지':'PDF',extensions:images?['jpg','jpeg','png','webp']:['pdf']}]}); if(!v)return;const arr=Array.isArray(v)?v:[v];if(images||multiple)setFiles(cur=>[...cur,...arr.filter(x=>!cur.includes(x))].slice(0,100));else setSource(arr[0]);setError('') } catch(e){setError(`파일 선택 실패: ${errorText(e)}`)} }
  async function run() {
    setBusy(true); setError(''); setResult(null)
    try {
      let r:Result
      if (['parity','rotate','delete','reorder'].includes(mode)) {
        if(!source) throw new Error('PDF 파일을 선택하세요.')
        const output=await save({title:'PDF 저장',defaultPath:`${mode}.pdf`,filters:[{name:'PDF',extensions:['pdf']}]}); if(!output)return
        if(mode==='parity') r=await invoke<Result>('pdf_parity',{path:source,odd:parityOdd,output})
        else if(mode==='rotate') r=await invoke<Result>('pdf_rotate',{path:source,ranges,degrees,output})
        else if(mode==='delete') r=await invoke<Result>('pdf_delete',{path:source,ranges,output})
        else r=await invoke<Result>('pdf_reorder',{path:source,ranges,output})
      } else if(mode==='merge') { if(files.length<2)throw new Error('병합할 PDF를 2개 이상 선택하세요.');const output=await save({title:'병합 PDF 저장',defaultPath:'merged.pdf',filters:[{name:'PDF',extensions:['pdf']}]});if(!output)return;r=await invoke<Result>('pdf_merge',{paths:files,output})
      } else if(mode==='extract') { if(!source)throw new Error('PDF 파일을 선택하세요.');const output=await save({title:'페이지 추출 PDF 저장',defaultPath:'extracted.pdf',filters:[{name:'PDF',extensions:['pdf']}]});if(!output)return;r=await invoke<Result>('pdf_extract',{path:source,ranges,output})
      } else if(mode==='split') { if(!source)throw new Error('PDF 파일을 선택하세요.');const output=await open({title:'분할 파일 저장 폴더 선택',directory:true,multiple:false});if(typeof output!=='string')return;r=await invoke<Result>('pdf_split',{path:source,outputDir:output})
      } else { if(!files.length)throw new Error('이미지를 선택하세요.');const output=await save({title:'이미지 PDF 저장',defaultPath:'images.pdf',filters:[{name:'PDF',extensions:['pdf']}]});if(!output)return;r=await invoke<Result>('pdf_images',{paths:files,output,widthMm:Number(width),heightMm:Number(height),marginMm:Number(margin)}) }
      setResult(r)
    } catch(e) { setError(errorText(e)) } finally { setBusy(false) }
  }
  function move(i:number,d:number){setFiles(current=>{const next=[...current],j=i+d;if(j<0||j>=next.length)return next;[next[i],next[j]]=[next[j],next[i]];return next})}
  const modes:[Mode,string][]=[['merge','PDF 병합'],['extract','페이지 추출'],['split','페이지별 분할'],['parity','홀수·짝수 추출'],['rotate','페이지 회전'],['delete','페이지 삭제'],['reorder','페이지 재정렬'],['images','이미지 → PDF']]
  const imageMode=mode==='images'
  return <div className="pdf-tools"><div className="page-intro"><div><div className="eyebrow"><Files size={14}/>기기에서 처리</div><h1>PDF 도구</h1><p>PDF를 병합·추출·분할하거나 이미지를 PDF로 저장합니다. 원본 파일은 보존됩니다.</p></div><span className="available-pill"><span/>사용 가능</span></div>
    <section className="operation-card"><h3>작업 선택</h3><div className="pdf-mode-tabs">{modes.map(([id,label])=><button key={id} className={`button ${mode===id?'button-primary':'button-secondary'}`} onClick={()=>{setMode(id);setFiles([]);setError('');setResult(null)}}>{label}</button>)}</div>
    {mode==='merge'&&<><p>선택한 순서대로 병합합니다. 위·아래 버튼으로 페이지 순서를 조정하세요. PDF 최대 100개, 결과 2,000페이지까지 지원합니다.</p><button className="button button-secondary" onClick={()=>void pick(true)}><FilePlus2 size={15}/>PDF 추가</button><FileList files={files} move={move} remove={p=>setFiles(f=>f.filter(x=>x!==p))}/></>}
    {['extract','split','parity','rotate','delete','reorder'].includes(mode)&&<><p>{mode==='split'?'선택한 PDF의 각 페이지를 별도 파일로 저장합니다.':mode==='parity'?'문서 순서 기준 홀수 또는 짝수 페이지를 추출합니다.':mode==='reorder'?'모든 페이지 번호를 새 순서대로 입력하세요 (예: 3,1,2).':mode==='delete'?'삭제할 페이지 번호와 범위를 입력하세요 (최소 한 페이지는 남겨야 합니다).':'페이지 번호와 범위를 쉼표로 입력하세요 (예: 1-3,5).'}</p><button className="button button-secondary" onClick={()=>void pick(false)}><FolderOpen size={15}/>PDF 선택</button>{source&&<p className="selected-folder">{source}</p>}{mode==='parity'&&<label><input type="radio" checked={parityOdd} onChange={()=>setParityOdd(true)}/> 홀수 페이지 <input type="radio" checked={!parityOdd} onChange={()=>setParityOdd(false)}/> 짝수 페이지</label>}{['extract','rotate','delete','reorder'].includes(mode)&&<label className="pdf-range">{mode==='reorder'?'새 페이지 순서':'페이지 범위'}<input value={ranges} onChange={e=>setRanges(e.target.value)} maxLength={2000} placeholder="예: 1-3,5"/></label>}{mode==='rotate'&&<label>회전 각도<select value={degrees} onChange={e=>setDegrees(Number(e.target.value))}><option value={90}>90°</option><option value={180}>180°</option><option value={270}>270°</option></select></label>}</>}
    {imageMode&&<><p>JPG/JPEG, PNG, WebP 파일을 한 장씩 A4 또는 사용자 지정 용지에 배치합니다. 비율을 유지하며 여백 안에 맞춥니다.</p><button className="button button-secondary" onClick={()=>void pick(true,true)}><FileImage size={15}/>이미지 추가</button><FileList files={files} move={move} remove={p=>setFiles(f=>f.filter(x=>x!==p))}/><div className="form-grid"><label>용지 너비 (mm)<input type="number" min="20" max="1000" value={width} onChange={e=>setWidth(e.target.value)}/></label><label>용지 높이 (mm)<input type="number" min="20" max="1000" value={height} onChange={e=>setHeight(e.target.value)}/></label><label>여백 (mm)<input type="number" min="0" max="500" value={margin} onChange={e=>setMargin(e.target.value)}/></label></div></>}
    <p className="image-note">각 입력 파일 최대 250MB, 이미지 최대 40MP, 작업당 최대 100개 및 PDF 2,000페이지입니다. 암호화된 PDF는 지원하지 않으며 기존 출력 파일을 덮어쓰지 않습니다.</p><button className="button button-primary" disabled={busy} onClick={()=>void run()}><Play size={15}/>{busy?'처리 중…':'실행 및 저장'}</button>{error&&<p className="error-message" role="alert">{error}</p>}</section>
    {result&&<section className="operation-card" role="status"><h3>작업 완료</h3><p>{result.fileCount}개 파일 · {result.pageCount}페이지</p><p className="selected-folder">저장 위치: {result.output}</p></section>}
  </div>
}
function FileList({files,move,remove}:{files:string[];move:(i:number,d:number)=>void;remove:(p:string)=>void}) { return files.length>0?<div className="image-file-list">{files.map((p,i)=><div key={p}><span className="count-badge">{i+1}</span><span title={p}>{basename(p)}</span><button aria-label="위로 이동" disabled={i===0} onClick={()=>move(i,-1)}><ArrowUp size={14}/></button><button aria-label="아래로 이동" disabled={i===files.length-1} onClick={()=>move(i,1)}><ArrowDown size={14}/></button><button aria-label="목록에서 제거" onClick={()=>remove(p)}><Trash2 size={14}/></button></div>)}</div>:null }
