import './App.css'
import { LexicalComposer } from '@lexical/react/LexicalComposer'
import { RichTextPlugin } from '@lexical/react/LexicalRichTextPlugin'
import { ContentEditable } from '@lexical/react/LexicalContentEditable'
import { HistoryPlugin } from '@lexical/react/LexicalHistoryPlugin'
import { LexicalErrorBoundary } from '@lexical/react/LexicalErrorBoundary'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { $getRoot, $createParagraphNode, $createTextNode } from 'lexical'
import { HeadingNode, $createHeadingNode } from '@lexical/rich-text'
import { $getSelection, $isRangeSelection, FORMAT_TEXT_COMMAND } from 'lexical'
import { useCallback, useEffect, useState } from 'react'
import { $setBlocksType } from '@lexical/selection'
import { $isHeadingNode } from '@lexical/rich-text'
import { ListPlugin } from '@lexical/react/LexicalListPlugin'
import { UNDO_COMMAND, REDO_COMMAND } from 'lexical'
import {
  ListNode,
  ListItemNode,
  INSERT_UNORDERED_LIST_COMMAND,
  REMOVE_LIST_COMMAND,
  $isListNode
} from '@lexical/list'
import {
  $getNodeByKey,
  type ElementNode
} from 'lexical'


function EditablePlugin({ isEditing }: { isEditing: boolean }) {
  const [editor] = useLexicalComposerContext()

  useEffect(() => {
    editor.setEditable(isEditing)
  }, [editor, isEditing])

  return null
}


function ToolbarPlugin() {
  const [editor] = useLexicalComposerContext()
  const [isBold, setIsBold] = useState(false)
  const [isItalic, setIsItalic] = useState(false)
  const [isHeading, setIsHeading] = useState(false)
  const [isBulletList, setIsBulletList] = useState(false)


  const updateToolbar = useCallback(() => {
    const selection = $getSelection()

    if ($isRangeSelection(selection)) {
      setIsBold(selection.hasFormat('bold'))
      setIsItalic(selection.hasFormat('italic'))
      const anchorNode = selection.anchor.getNode()
      const topLevelElement = anchorNode.getTopLevelElementOrThrow()

      let currentNode = anchorNode.getParent()

      while (currentNode && !$isListNode(currentNode)) {
        currentNode = currentNode.getParent()
      }

      setIsBulletList(
        $isListNode(currentNode) &&
        currentNode.getListType() === 'bullet'
      )

      setIsHeading(
        $isHeadingNode(topLevelElement) &&
        topLevelElement.getTag() === 'h2'
      )
    } else {
      setIsBold(false)
      setIsItalic(false)
      setIsHeading(false)
      setIsBulletList(false)
    }
  }, [])

  useEffect(() => {
    return editor.registerUpdateListener(({ editorState }) => {
      editorState.read(updateToolbar)
    })
  }, [editor, updateToolbar])

  return (
    <nav className="editor-toolbar" aria-label="Textformatering">
      <button
        type="button"
        className={isBold ? 'active' : ''}
        aria-label="Fetstil"
        aria-pressed={isBold}
        onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'bold')}
      >
        <strong>B</strong>
      </button>

      <button
        type="button"
        className={isItalic ? 'active' : ''}
        aria-label="Kursiv stil"
        aria-pressed={isItalic}
        onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'italic')}
      >
        <em>I</em>
      </button>

      <button
        type="button"
        className={isHeading ? 'active' : ''}
        aria-label="Rubrik"
        aria-pressed={isHeading}
        onClick={() => {
          editor.update(() => {
            const selection = $getSelection()

            if ($isRangeSelection(selection)) {
              $setBlocksType(
                selection,
                () => isHeading
                  ? $createParagraphNode()
                  : $createHeadingNode('h2')
              )
            }
          })
        }}
      >
        H2
      </button>


      <button
        type="button"
        className={isBulletList ? 'active' : ''}
        aria-label="Punktlista"
        aria-pressed={isBulletList}
        onClick={() => {
          editor.dispatchCommand(
            isBulletList
              ? REMOVE_LIST_COMMAND
              : INSERT_UNORDERED_LIST_COMMAND,
            undefined
          )
        }}
      >
        ☷
      </button>

      <span className="toolbar-divider" aria-hidden="true" />

      <button
        type="button"
        aria-label="Ångra"
        onClick={() => editor.dispatchCommand(UNDO_COMMAND, undefined)}
      >
        ↶
      </button>

      <button
        type="button"
        aria-label="Gör om"
        onClick={() => editor.dispatchCommand(REDO_COMMAND, undefined)}
      >
        ↷
      </button>


    </nav>
  )
}


const initialConfig = {
  namespace: 'MNOJournal',
  theme: {},
  nodes: [HeadingNode, ListNode, ListItemNode],
  onError(error: Error) {
    throw error
  },
  editorState: () => {
    const root = $getRoot()

    const paragraph1 = $createHeadingNode('h2')
    paragraph1.append(
      $createTextNode('En stilla morgon vid sjön')
    )

    const paragraph2 = $createParagraphNode()
    paragraph2.append(
      $createTextNode(
        'Dimman låg fortfarande över vattnet när jag kom fram till stranden.'
      )
    )

    const paragraph3 = $createParagraphNode()
    paragraph3.append(
      $createTextNode(
        'En rödhake sjöng från buskarna och längre bort hördes en spillkråka. Det var en sådan morgon då naturen kändes alldeles nära.'
      )
    )

    root.append(paragraph1, paragraph2, paragraph3)
  }
}

export default function App() {
  const [isEditing, setIsEditing] = useState(false)

  return (
    <main className="journal-page">
      <header className="journal-header">
        <div>
          <p className="eyebrow">MY NATURE OBSERVATIONS</p>
          <p className="date">12 oktober 2026</p>
        </div>

        <button onClick={() => setIsEditing(!isEditing)}>
          {isEditing ? '✓ Klar' : 'Redigera'}
        </button>
      </header>

      <article className="journal-entry">
        <LexicalComposer initialConfig={initialConfig}>
          <EditablePlugin isEditing={isEditing} />
          {isEditing && <ToolbarPlugin />}

          <RichTextPlugin
            contentEditable={
              <ContentEditable
                className="editor-content"
                aria-label="Naturdagboksanteckning"
              />
            }
            ErrorBoundary={LexicalErrorBoundary}
          />

          <HistoryPlugin />
          <ListPlugin />
        </LexicalComposer>
      </article>

      <footer className="journal-footer">
        Prototyp – ingen permanent lagring
      </footer>
    </main>
  )
}
