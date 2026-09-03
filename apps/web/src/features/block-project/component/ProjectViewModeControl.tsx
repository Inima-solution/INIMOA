import { SegmentedControl } from '@ui';

export type ProjectTaskViewMode = 'list' | 'board' | 'timeline' | 'decisions';

export function ProjectViewModeControl(props: {
  mode: ProjectTaskViewMode;
  onChange: (mode: ProjectTaskViewMode) => void;
  density?: 'desktop' | 'touch';
}) {
  const isTouch = () => props.density === 'touch';

  return (
    <SegmentedControl
      value={props.mode}
      onChange={props.onChange}
      aria-label="Project view"
      size="sm"
      class={
        isTouch()
          ? 'project-view-mode-control shrink-0 **:min-h-11'
          : 'project-view-mode-control shrink-0 **:min-h-10 touch:**:min-h-11'
      }
      options={[
        { value: 'list', label: 'List' },
        { value: 'board', label: 'Board' },
        { value: 'timeline', label: 'Timeline' },
        { value: 'decisions', label: 'Decisions' },
      ]}
    />
  );
}
