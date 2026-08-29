import {Composition} from "remotion";
import {OmadesignVideo} from "./OmadesignVideo";

export const RemotionRoot = () => {
  return (
    <>
      <Composition
        id="Omadesign"
        component={OmadesignVideo}
        durationInFrames={1170}
        fps={30}
        width={1920}
        height={1080}
        defaultProps={{}}
      />
    </>
  );
};
