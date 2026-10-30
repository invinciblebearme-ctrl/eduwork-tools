export type PageId =
  | 'home'
  | 'pdf'
  | 'excel'
  | 'image'
  | 'files'
  | 'convert'
  | 'privacy'
  | 'automation'
  | 'settings'

export interface SelectedPath {
  path: string
  kind: 'file' | 'folder'
}
export interface FileEntry { path: string; name: string; size: number; extension: string; modified: string; isDirectory: boolean }
export interface RenamePlan { source: string; target: string; conflict: boolean }
