//
//  ViewController.swift
//  VexoDemo
//
//  Created by peiyan_wang on 2025/11/30.
//

import UIKit
import shared_app
import MetalKit

class MtkViewContainer: MTKView {
    var uiEngine: MobileApp = MobileApp()

    required init(coder: NSCoder) {
        fatalError()
    }
    
    override init(frame frameRect: CGRect, device: (any MTLDevice)?) {
        super.init(frame: frameRect, device: device)
        self.setupMtkView()
        self.setupTapGesture()
        self.setupUiEngine()
    }
    
    private func setupUiEngine() {
        let layerPtr = unsafeBitCast(self.layer, to: UInt64.self)
        let scale = self.traitCollection.displayScale
        print("scale: \(scale)")
        
        let width = UInt32(self.bounds.size.width)
        let height = UInt32(self.bounds.size.height)
        uiEngine.initRenderer(viewPtrAsU64: layerPtr, width: width, height: height, scaleFactor: Float(scale))
        self.mtkView(self, drawableSizeWillChange: self.drawableSize)
    }
    
    private func setupMtkView() {
        self.delegate = self
        self.clearColor = MTLClearColor(
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0
        )
    }
    
    private func setupTapGesture() {
        let tapGesture = UITapGestureRecognizer(target: self, action: #selector(self.handleTap(_:)))
        self.addGestureRecognizer(tapGesture)
    }
    
    @objc func handleTap(_ sender: UITapGestureRecognizer) {
        // 1. Get the tap location in the view's coordinate system (points).
        let locationInView = sender.location(in: self)
        print("mtk view container tap: \(locationInView)")
        uiEngine.onTap(x: Float(locationInView.x), y: Float(locationInView.y))
    }
}

extension MtkViewContainer: MTKViewDelegate {
    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
        print("mtk view size change: \(size)")
        uiEngine.resize(width: Float(size.width), height: Float(size.height))
        view.setNeedsDisplay()
    }
    
    func draw(in view: MTKView) {
        uiEngine.render()
    }
}

class ViewController: UIViewController {
    
    private var mtkViewContainer: MtkViewContainer!
    
    override func viewDidLoad() {
        super.viewDidLoad()
        mtkViewContainer = MtkViewContainer(
            frame: self.view.bounds,
            device: MTLCreateSystemDefaultDevice()
        )
        mtkViewContainer.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        self.view.addSubview(mtkViewContainer)
        self.view.backgroundColor = .black
    }
}

