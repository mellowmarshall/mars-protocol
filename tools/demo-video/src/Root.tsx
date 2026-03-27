import React from "react";
import { Composition } from "remotion";
import { MarsDemo } from "./MarsDemo";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="MarsDemo"
        component={MarsDemo}
        durationInFrames={38 * 30} // 38 seconds at 30fps
        fps={30}
        width={1280}
        height={720}
      />
    </>
  );
};
