#version 460 core

in vec2 vUv;
out vec4 fragColor;

uniform sampler2D uTexture;

void main() {
    fragColor = texture(uTexture, vUv);
}
