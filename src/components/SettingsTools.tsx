import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

type AppInfo = { name: string; version: string; phase: string }
const PREF_KEY = 'eduwork.settings.outputFolder'
export function SettingsTools() {
  const [info, setInfo] = useState<AppInfo | null>(null)
  const [folder, setFolder] = useState(() => localStorage.getItem(PREF_KEY) ?? '')
  useEffect(() => { void invoke<AppInfo>('app_info').then(setInfo).catch(() => setInfo(null)) }, [])
  function saveFolder(value: string) {
    setFolder(value)
    if (value.trim()) localStorage.setItem(PREF_KEY, value.trim())
    else localStorage.removeItem(PREF_KEY)
  }
  return <div className="file-tools settings-tools">
    <div className="page-intro"><div><h1>설정</h1><p>이 화면의 환경설정과 앱 데이터는 이 기기의 브라우저 저장소에만 보관됩니다.</p></div></div>
    <section className="operation-card"><h3>앱 정보</h3><p>{info ? `${info.name} · 버전 ${info.version}` : '앱 정보를 불러올 수 없습니다.'}</p><p>기능 단계: {info?.phase ?? '확인 불가'}</p></section>
    <section className="operation-card"><h3>로컬 기본 설정</h3><label>기본 출력 폴더 (메모)<input aria-label="기본 출력 폴더" value={folder} onChange={e => saveFolder(e.target.value)} placeholder="예: C:\\Users\\사용자\\Documents\\EduWork" /></label><p>경로를 기억하는 메모입니다. 현재 작업 화면의 폴더 선택을 자동으로 바꾸지는 않습니다.</p></section>
    <section className="operation-card"><h3>개인정보 및 데이터</h3><p>문서 변환·검사와 파일 작업은 로컬에서 실행됩니다. 네트워크 전송이나 사용 분석 설정은 제공하지 않으며, 이 화면은 이를 켜거나 끄는 가짜 스위치를 두지 않습니다.</p><p>워크플로 템플릿과 실행 기록은 앱 데이터의 workflows.json에 저장됩니다. 앱을 종료한 뒤 파일을 별도 위치에 복사해 백업하고, 복원 전에는 현재 파일을 다른 이름으로 보관하세요. 앱 내부 백업/복원 기능은 제공하지 않습니다.</p></section>
  </div>
}
