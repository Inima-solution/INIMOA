import { SegmentedControl } from '@ui';

export type ProjectTaskViewMode = 'list' | 'board';

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
      aria-label="Project task view"
      size="sm"
      class={
        isTouch()
          ? 'project-view-mode-control shrink-0 **:min-h-11'
          : 'project-view-mode-control shrink-0 **:min-h-10 touch:**:min-h-11'
      }
      options={[
        { value: 'list', label: 'List' },
        { value: 'board', label: 'Board' },
      ]}
    />
  );
}
