# frozen_string_literal: true

module Inkoc
  module Codegen
    class Instruction
      include Inspect

      NAME_MAPPING = %i[
        Allocate
        AllocatePermanent
        ArrayAllocate
        ArrayAt
        ArrayClear
        ArrayLength
        ArrayRemove
        ArraySet
        AttributeExists
        BlockGetReceiver
        ByteArrayAt
        ByteArrayClear
        ByteArrayEquals
        ByteArrayFromArray
        ByteArrayLength
        ByteArrayRemove
        ByteArraySet
        ByteArrayToString
        Close
        CopyBlocks
        CopyRegister
        Exit
        ExternalFunctionCall
        ExternalFunctionLoad
        FloatAdd
        FloatCeil
        FloatDiv
        FloatEquals
        FloatFloor
        FloatGreater
        FloatGreaterOrEqual
        FloatIsInfinite
        FloatIsNan
        FloatMod
        FloatMul
        FloatRound
        FloatSmaller
        FloatSmallerOrEqual
        FloatSub
        FloatToBits
        FloatToInteger
        FloatToString
        GeneratorAllocate
        GeneratorResume
        GeneratorValue
        GeneratorYield
        GetAttribute
        GetAttributeInSelf
        GetAttributeNames
        GetBuiltinPrototype
        GetFalse
        GetGlobal
        GetLocal
        GetNil
        GetParentLocal
        GetPrototype
        GetTrue
        Goto
        GotoIfFalse
        GotoIfTrue
        IntegerAdd
        IntegerBitwiseAnd
        IntegerBitwiseOr
        IntegerBitwiseXor
        IntegerDiv
        IntegerEquals
        IntegerGreater
        IntegerGreaterOrEqual
        IntegerMod
        IntegerMul
        IntegerShiftLeft
        IntegerShiftRight
        IntegerSmaller
        IntegerSmallerOrEqual
        IntegerSub
        IntegerToFloat
        IntegerToString
        LocalExists
        ModuleGet
        ModuleLoad
        MoveResult
        ObjectEquals
        Panic
        ProcessAddDeferToCaller
        ProcessCurrent
        ProcessIdentifier
        ProcessReceiveMessage
        ProcessSendMessage
        ProcessSetBlocking
        ProcessSetPanicHandler
        ProcessSetPinned
        ProcessSpawn
        ProcessSuspendCurrent
        ProcessTerminateCurrent
        Return
        RunBlock
        RunBlockWithReceiver
        SetAttribute
        SetBlock
        SetDefaultPanicHandler
        SetGlobal
        SetLiteral
        SetLiteralWide
        SetLocal
        SetParentLocal
        StringByte
        StringConcat
        StringConcatArray
        StringEquals
        StringFormatDebug
        StringLength
        StringSize
        StringSlice
        StringToByteArray
        StringToFloat
        StringToInteger
        StringToLower
        StringToUpper
        TailCall
        Throw
      ]
        .each_with_index
        .each_with_object({}) { |(value, index), hash| hash[value] = index }
        .freeze

      attr_reader :index, :arguments, :location

      def self.named(name, arguments, location)
        new(NAME_MAPPING.fetch(name), arguments, location)
      end

      def initialize(index, arguments, location)
        @index = index
        @arguments = arguments
        @location = location
      end

      def line
        @location.line
      end
    end
  end
end
